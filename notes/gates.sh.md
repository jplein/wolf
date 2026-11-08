# `gates.sh`

Last verified at commit `d84329823c43b042bcaac6a5f67c496f4bdef51b`.

## Dev shell entry

Every gate needs the flake's dev shell, which pins the toolchain each platform's gates call. So `gates.sh` runs the gates directly only when `WOLF_DEV_SHELL=1`, which both shellHooks in `flake.nix` export, and otherwise runs itself once more under `nix develop --command`. The marker is Wolf's own because any flake's shell sets `IN_NIX_SHELL`, so a run started inside another project's shell would skip entering this one. Regina already wraps each gate of a `workflow = nix` project in `nix develop`, so there `gates.sh` starts inside the shell and must not wrap again.

The second run is a child rather than an `exec`, so the first one outlives it. It removes `target/gates` before entering, and a directory still missing afterwards means `nix develop` or the inner run failed before any gate ran, usually at the loop guard below, which it reports as exit 2. `WOLF_GATES_REENTERED` guards against a loop: an inner run that sees it without the marker exits 2 rather than entering again, since only a shellHook that lost the export gets there.

The marker is trusted, never checked against `flake.nix`, so a direnv environment loaded before an edit to the flake carries it without the edit. [build.md](../build.md#the-verification-gate) has the command for that case.

`/usr/bin/env bash` outside the dev shell on macOS is bash 3.2, so the script avoids `mapfile`, associative arrays and `wait -n`, and never expands an empty array under `set -u`.

## Temporary directory

On Linux the gates run with `TMPDIR` set to a fresh `/tmp/wolf-gates.XXXXXX`, removed on exit. The cage smokes in `crates/wolf-gnome/tests/smoke_headless.rs` make each compositor's runtime directory under `TMPDIR` and assert that its path fits `RUNTIME_DIR_MAX_BYTES`, because the Wayland socket inside it must fit `sun_path`. `nix develop` nests `TMPDIR` under its own `nix-shell.XXXXXX`, 17 bytes of that budget, so the base is always `/tmp` and never the inherited `TMPDIR`.

The outer run makes the directory before entering the shell, so the directories `nix develop` leaves behind land inside it and go with it. It passes the path on as `WOLF_GATES_TMPDIR`, from which the inner run restores `TMPDIR`. An inner run started without one, as under regina, makes its own.

On macOS `gates.sh` leaves `TMPDIR` alone, and `make_tmpdir` must stay Linux-only. No gate there has a path budget, and regina's Seatbelt profile grants a worker its own `TMPDIR` and denies `/tmp`. A directory removed on exit would also be pulled from under the Bazel server, which outlives the run. The cost is that a run started outside the dev shell leaves the one `nix-shell.*` directory `nix develop` makes under the inherited `TMPDIR`.

## Gate order

Every gate runs even after one fails, so one run reports every failure, and they run fast to slow, so a formatting slip shows within seconds. The format and Markdown checks compile nothing. doc-lint is a small crate with no dependencies, so its gates come next. `wolf-core-deps` runs just before the first gate that compiles core's dependency graph, `clippy` on Linux and `lint` on macOS, so a forbidden dependency fails in seconds and the graph it fetches is the one that gate reuses. clippy precedes the build because it compiles the dependency graph the build then reuses, and the tests run last because they dominate the wall clock.

Prettier's scope, on both platforms, is `**/*.md` minus `.prettierignore`. The glob is quoted so prettier expands it rather than the shell, whose `**` without `globstar` descends one directory only. Prettier 3.9.10 follows no symlinked directory while it expands that glob: with the ignore file disabled it reports nothing under `.direnv/` or the `bazel-*` links, whose Markdown it flags when named directly, and the check takes about a second on macOS.

`run` takes one command, so each platform's `wolf-core-deps` pipeline is a function, `core_deps_cargo` or `core_deps_bazel`, which the log's first line names. Under `pipefail` a producer that fails fails the gate, and the checker exits 2 on an empty listing, so a listing that never arrived cannot pass as clean.

## Linux gates

`--all-targets` on clippy is what makes it lint `tests/*.rs` and every `#[cfg(test)] mod tests` rather than only the libraries and binaries. `cargo test` omits it deliberately, because that flag drops the doctests, and takes `--no-fail-fast` so a failing test binary does not hide the ones after it.

doc-lint is a Cargo workspace of its own, so `--workspace` never reaches it and its gates name its manifest.

`wolf-core-deps` feeds the checker crate names only: `{p}` prints a path dependency with its checkout path, which `cut` drops so that no pattern can match the worker's directory. `--target all` is there because the one breach this rule has met, notify 8.2.1's `objc2-core-*` crates, is a macOS-only dependency that the Linux host-target tree never lists, at the cost of a cold registry also fetching the Windows and wasm crates in the lock. The checker's own test, which compiles nothing, runs as `wolf-core-deps-check-test` beside the Markdown checker's test, and on macOS under `lint`.

`./tools/wolf-package-smoke.sh` stays out of the gate. Its package is input-addressed over everything under `crates/`, so after any source edit it is a cold release compile with no `target/` reuse, on top of the gate's own build. It runs on its own, as [build.md](../build.md#the-nix-package) describes.

## macOS gates

`bazel test //...` runs every target not tagged `manual`: the `//tools/lint` checks, the crate tests, and `:Wolf` through `bundle_layout_test`. Prettier has no Bazel target, so it is a gate of its own. The `:comments` target runs doc-lint per file, and its checkout-wide rules — `md-link`, `md-anchor`, `notes-orphan`, `notes-unlinked`, `bazel-coverage` — run only from the `doc-lint` gate's `--root`.

`lint` runs `//tools/lint:check` alone, so a lint failure is named as one, and `build` separates a compile failure from a test failure; `test` reuses both. `WolfRelease` and `InstallWolf` stay out: both are `manual`, and the first rebuilds the whole graph at `-c opt`.

`wolf-core-deps` queries `kind(rule, deps(//crates/wolf-core:wolf_core))`, and the `kind(rule, …)` wrapper is load-bearing. A bare `deps()` also returns every source file in the graph, and `libc` vendors its bindings under `src/new/glibc/…`, whose paths match the `glib` pattern, so the gate would fail on every run. crate_universe resolves only the darwin triples in `MODULE.bazel`, so this gate covers the macOS graph alone, and Linux's `--target all` covers every target.

`--keep_going` reports every failing target rather than the first. `--color=no`, `--curses=no` and a 30-second progress rate keep the log plain and bounded, and `--test_output=errors` copies each failing test's log into the gate log.

## Failure output

A remote regina worker returns only the gate's stdout and stderr, never `target/gates/`, so each failure must be diagnosable from stdout. A log of up to 150 lines is printed whole. A longer one is printed as an excerpt, then its last 150 lines: every line matching a pattern, with 40 lines of context, capped at 400.

The patterns are a failing cargo test's `---- <test> stdout ----`, rustc's and cargo's `error:` and `error[E…]:`, Bazel's `ERROR:` and `FAIL:`, Bazel's `==================== Test output for` block, rustfmt's `Diff in` and prettier's `[warn]`. Each gate reads `/dev/null`, so one that prompts fails instead of hanging.
