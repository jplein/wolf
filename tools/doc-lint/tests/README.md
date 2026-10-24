# doc-lint/tests

Integration tests for [doc-lint](../README.md). All are headless: no Neovim and no GUI, only the public API of the `doc_lint` library run against fixture files.

```sh
cargo test --manifest-path tools/doc-lint/Cargo.toml   # Linux gate
bazel test //tools/doc-lint/...                        # macOS
```

## Adding a test file

Cargo discovers every `tests/*.rs`, but Bazel does not: a new file needs its own `rust_test` in [`../BUILD.bazel`](../BUILD.bazel). Files under `fixtures/` are data and are never compiled, since Cargo only looks for `main.rs` in a test subdirectory.

## Fixtures

- `fixtures/lang/<language>/` holds one or more files per language, each beside a `<file>.expected` sidecar. Each region of a fixture opens with a one-line `case:` comment naming what it covers, and those comments obey the limits themselves.
- `fixtures/lang/other/notes.txt` is the no-language control and has no sidecar.
- `fixtures/repo/` is a miniature repository for `check_root`. Its `docs/history.md` spells workflow references in Markdown, which must not be reported, beside the one in `src/history.rs`, which must.
- `fixtures/same_code/` holds `strip` and `same-code` pairs, each compared with `old.rs` or `old.nix`.
- `fixtures/cli/` holds inputs for `run()`. `workflow_ref.rs` mixes upper and lower case so the quoted match shows which copy it came from.
- `fixtures/files_mode/` is a miniature repository root for the `--files` test. Its `src/notes/` is a decoy, not a mirror.

A sidecar has one `LINE RULE` pair per line, such as `7 comment-lines`, and an empty sidecar means no findings. The comparison is exact, so a duplicate report is a mismatch.

**Decisions**

- No fixture is named `BUILD.bazel`, `BUILD`, `MODULE.bazel`, `Cargo.toml`, `flake.nix`, `.envrc` or `.gitignore`, and no fixture directory is named `target`, because Bazel, Cargo, Nix, direnv or git would treat it as a real file. `api.rs` tests those mappings instead.
- A fixture file cannot carry trailing whitespace, which editors and formatters strip, so `fixtures.rs` builds that case in code.

## Files

#### `fixtures.rs`

Purpose: the data-driven runner over `fixtures/`, plus the repo-mode, `same-code` and CLI tests.

**Covers**

- Every `lang/` fixture against its sidecar through `lint_source`. It fails when there are no fixtures, a fixture lacks a sidecar, a sidecar is stray, a `Language` variant has no fixture, or the `notes.txt` control is missing.
- `check_root` over `repo/`: the exact `(path, line, rule)` set and the wording of the `doc-ref`, `doc-anchor`, `notes-orphan`, `notes-unlinked` and `workflow-ref` messages.
- `strip_comments` and the `same-code` subcommand over `same_code/`; `run()` exit codes, the exact finding line, and path order.
- `--files` running `workflow-ref` over `cli/workflow_ref.rs`, quoting each match in its source case.

**Gotchas**

- The fixture root comes from `DOC_LINT_FIXTURES` under Bazel, relative to the runfiles root, and from `CARGO_MANIFEST_DIR` otherwise. The CLI tests pass cwd-relative paths, relying on Cargo running tests from the package root.
- The repo test copies `repo/` to a scratch directory first, because Bazel runfiles are symlinks and the walk never follows them. It creates `docs/dir with space/`, `node_modules/`, `crates/wolf-ffi/generated/` and two symlinks there at run time, which Bazel runfiles or a global gitignore could not carry.

#### `files_mode.rs`

Purpose: `run()` in `--files` mode, checking that `doc-ref` targets resolve against the working directory, over `fixtures/files_mode/`.

**Decisions**

- It is a separate test binary holding one test, because the test changes the process cwd, which would race with any other test in the same process. Spawning the built binary instead would need `CARGO_BIN_EXE_doc-lint`, which only Cargo provides; Bazel would need the binary in `data` plus an env var.
- The fixture's `src/` holds a decoy of the missing target and lacks the present one, so resolving against the file's directory moves the one finding to another line rather than hiding it.

#### `api.rs`

Purpose: `language_for` name mappings, `heading_slug`, the `workflow-ref` rule id, and `missing_coverage` against an inline stand-in for the lint package's `BUILD.bazel`. Needs no fixture data.
