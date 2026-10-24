# tools/doc-lint

`doc-lint` enforces the comment limits in [CLAUDE.md](../../CLAUDE.md), flags comments that cite planning documents absent from the repo, checks the doc paths written in comments and the `notes/` mirror conventions, and, over a whole checkout, checks relative links in Markdown. It is a std-only Rust binary in a Cargo workspace of its own. The per-file view of the source is in [`src/README.md`](src/README.md), and the tests are described in [`tests/README.md`](tests/README.md).

## Command line

```sh
doc-lint --root <dir>            # every rule over the tree under <dir>
doc-lint --files <path>...       # comment limits, workflow-ref, doc-ref and doc-anchor, relative to the cwd
doc-lint strip <file>            # the file with every comment removed
doc-lint same-code <old> <new>   # exit 0 when the two differ only in comments
```

Findings go to stdout as `path:line: [rule] message`, sorted by path and then line, and a clean run prints nothing there. Exit status 0 is clean, 1 is findings or, for `same-code`, a code difference, and 2 is a usage or I/O error. Paths are root-relative under `--root` and printed as given, less a leading `./`, under `--files`; a `--files` path with no language is skipped.

`strip` removes every comment, doc comments included, and keeps strings verbatim; outside strings it trims trailing whitespace and drops blank lines, so deleting a comment block and the blank lines around it changes nothing. `same-code` strips both files with the language of `<new>`'s name and prints the first differing stripped line. It is how a comment-only edit proves it changed no code.

## Rules

| Rule             | Mode     | Fails on                                                                                                           | Message                                                                                                                                     |
| ---------------- | -------- | ------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `comment-lines`  | both     | A non-doc comment block over `MAX_COMMENT_LINES` (2) lines, reported at its first line.                            | `comment is N lines (max 2): cut it, shorten it, or move the rest to notes/<path>.md under a heading and link it`                           |
| `comment-width`  | both     | A non-doc comment line over `MAX_COMMENT_WIDTH` (100) chars.                                                       | `comment line is N chars (max 100), measured from the comment marker`                                                                       |
| `doc-ref`        | both     | A Markdown path in any comment, doc comments included, that does not exist.                                        | `<ref> does not exist`                                                                                                                      |
| `doc-anchor`     | both     | The `#anchor` of such a path, when it matches no heading of the file.                                              | `<file> has no heading with anchor #x`                                                                                                      |
| `workflow-ref`   | both     | A comment line, doc comments included, holding a pattern from [Workflow references](#workflow-references).         | `workflow reference "<matched text>": it cites a planning document that is not in the repo; state the reason in present tense or delete it` |
| `notes-orphan`   | `--root` | A mirror `notes/<P>.md` whose `<P>` does not exist.                                                                | `notes/<P>.md mirrors <P>, which does not exist`                                                                                            |
| `notes-unlinked` | `--root` | A `##` heading of a mirror that no comment in `<P>` links to.                                                      | `heading "<text>" (#slug) is not linked from any comment in <P>`                                                                            |
| `md-link`        | `--root` | A relative link, image or reference definition whose target does not exist.                                        | `<target> does not exist`                                                                                                                   |
| `md-anchor`      | `--root` | A link fragment that matches no heading of the target Markdown file.                                               | `<file> has no heading with anchor #x`                                                                                                      |
| `bazel-coverage` | `--root` | A `BUILD.bazel` whose `//<dir>:comment_srcs` or `//<dir>:markdown_srcs` is not listed in `tools/lint/BUILD.bazel`. | `tools/lint/BUILD.bazel does not list "<label>"`                                                                                            |

`bazel-coverage` runs only when the root has a `tools/lint/BUILD.bazel`, and the root package's labels are written `//:comment_srcs`. A label counts only when quoted in full.

### Comment limits

A full-line comment has only whitespace before its marker. Full-line line comments on consecutive lines with the same marker family and doc-ness form one block; Rust's `//`, `///` and `//!` are three families. A block comment is its own block, and so is a trailing comment, which never joins the comment lines after it.

Every physical line counts toward the limit, blank comment lines included, except a block comment's delimiter-only first or last line such as `/*`, `*/`, `--[[`, `]]`, or a Lua closer written with a leading `--`, as in `--]]` or `--]==]`. Width is counted in chars, trailing whitespace excluded, from the marker on a comment's first line and from the first non-space char on a block's later lines. A line whose text after the marker is a single token with no whitespace, such as a URL, is exempt from the width limit but still counts as a line.

There is no suppression comment. The exemptions are structural: doc comments, the single-token width exemption, and the excluded paths below.

### Workflow references

The patterns are `note to coder`, `note to the coder`, `the coder`, `the coders`, `test-writer`, `test writer`, `test-writers`, `test writers`, `architect plan`, `the architect`, `per plan`, `per the plan`, `from the plan`, `feature plan`, `fix plan`, `plan §`, `plan section`, and `OD` or `OQ` followed by digits or by `-` and ASCII letters or digits. Strings and Markdown are never scanned.

Matching ignores ASCII case, and a match end that is alphanumeric or `_` must not touch another such char. An id's digits or letters are taken greedily before that check, so `plan §4`, `OD2.`, `oq-a` and `method-OQ-1` match while `COD2`, `OD2B`, `OD2_x`, `OQ-1_x`, a bare `OD-`, `lathe coder` and `the architecture` do not. Each line gets at most one finding, even when two comments share it, quoting the leftmost match, the longest at that position, as the source spells it.

Bare `the plan`, `§`, and ids such as `D4`, `O2`, `H1` or `S1` are not patterns, because the code uses them as domain terms: restore plans, `CSS 2.1 §10.8`, `-O2`, file-local heights and labels.

### References and anchors

A reference starts at a line start, whitespace or one of `` ` ' " ( [ < ``, begins with `notes/`, `specs/`, `crates/`, `tools/`, `runtime/`, `bazel/` or `resources/`, runs over `[A-Za-z0-9_.@+/-]`, and is cut back to end in `.md`; an `#anchor` directly after it counts. A token without `.md`, such as the placeholder `notes/<crate>-decisions.md`, is not a reference, and neither is a path into an excluded prefix. References resolve against the root, or the cwd under `--files`.

Anchors follow GitHub: `#`-style headings outside fenced code, lowercased, with every char that is not alphanumeric, a space, `-` or `_` dropped and spaces turned into `-`; a repeated anchor takes `-1`, `-2` and so on. `heading_slug` computes one heading's anchor before that suffix.

Markdown links are inline links and images, including `<…>` destinations, and reference definitions, outside fenced and inline code. A target with a URI scheme such as `https:` or `mailto:` is skipped, a title is dropped and `%XX` escapes are decoded. A target resolves against the file's directory, or the root when it starts with `/`, and must exist as a file or a directory.

### Mirrors

`notes/<P>.md` is a mirror whenever `<P>`'s file name has a language, so `notes/src/a.rs.md` mirrors `src/a.rs`. Every `##` heading in a mirror must be the anchor of a reference to the mirror from a comment in `<P>`. Any other `notes/` file is free-form.

## Languages

| Language   | Files                                                         | Line marker            | Block comment            | Exempt doc forms                                                   |
| ---------- | ------------------------------------------------------------- | ---------------------- | ------------------------ | ------------------------------------------------------------------ |
| `Rust`     | `*.rs`                                                        | `//`                   | `/* */`, nesting         | `///` (not `////`), `//!`, `/** */` (not `/**/`, `/***`), `/*! */` |
| `Swift`    | `*.swift`                                                     | `//`                   | `/* */`, nesting         | `///`, `/** */`                                                    |
| `C`        | `*.c`, `*.h`                                                  | `//`                   | `/* */`                  | none                                                               |
| `Css`      | `*.css`                                                       | none                   | `/* */`                  | none                                                               |
| `Lua`      | `*.lua`                                                       | `--`, `---` included   | `--[[ ]]`, `--[==[ ]==]` | none                                                               |
| `Shell`    | `*.sh`, `*.sh.tpl`, `.envrc`                                  | `#` at a word start    | none                     | none                                                               |
| `Starlark` | `BUILD.bazel`, `BUILD`, `*.bzl`, `MODULE.bazel`, `WORKSPACE*` | `#`                    | none                     | none                                                               |
| `Nix`      | `*.nix`                                                       | `#`                    | `/* */`                  | none                                                               |
| `Toml`     | `*.toml`                                                      | `#`                    | none                     | none                                                               |
| `HashLine` | `.bazelrc`, `.bazelignore`, `.gitignore`, `.prettierignore`   | `#` as first non-space | none                     | none                                                               |

**The Bazel file list must match this table.** `COMMENT_LINT_PATTERNS` in [`tools/lint/defs.bzl`](../lint/defs.bzl) holds the patterns every package globs, and the root `BUILD.bazel` adds the root-only names. A file missing from the patterns is checked on Linux and skipped on macOS.

A file with no language is never read. Text inside a string is string content, so shell comments in a Nix `''` string, a generated `BUILD` file in a Starlark `"""` string, a heredoc body or a script in a Rust string are not linted.

## Exclusions

The walk never follows symlinks. It skips directories named `.git`, `target`, `.direnv`, `.worktrees`, `.claude`, `node_modules`, `.wolf`, `bazel-*` and `*.xcodeproj`, and the prefixes `crates/wolf-ffi/generated/`, `tools/doc-lint/tests/fixtures/` and `tools/lint/testdata/`, which are not references either. `WalkConfig::repo_default()` holds both lists.

## Building and testing

On Linux, from the repository root inside the devShell:

```sh
cargo build --manifest-path tools/doc-lint/Cargo.toml
cargo test --manifest-path tools/doc-lint/Cargo.toml
cargo clippy --manifest-path tools/doc-lint/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path tools/doc-lint/Cargo.toml -- --check
cargo run --quiet --manifest-path tools/doc-lint/Cargo.toml -- --root .
```

On macOS, `bazel test //tools/doc-lint/...` runs the tests, and `//tools/lint:comments` runs `doc-lint --files` over every package's `comment_srcs`. Both platforms gate on these; the full gate is in [`build.md`](../../build.md).

## Decisions

- Rust rather than awk, because finding comments takes per-language string lexing: raw strings, Swift interpolation, Nix `''` strings, Lua long brackets, and shell's `$#`, `${x#y}` and heredocs. The same lexer powers `same-code`.
- A workspace of its own with no dependencies, so the root `Cargo.lock` and the `crate_universe` repin, which only macOS can run, are untouched, and Bazel builds it with plain `rust_library`. It must compile on rules_rust's rustc 1.85, which is why `Cargo.toml` sets `rust-version` and the code has no let-chains.
- Width is measured from the marker, so indentation never pushes a comment over the limit.
- Markdown links are checked under `--root` only, because a Bazel test's runfiles hold only declared inputs, never every link target. For the same reason `//tools/lint:comments` stages every package's Markdown as data, since `--files` resolves references against the cwd, which under Bazel is the runfiles root.
