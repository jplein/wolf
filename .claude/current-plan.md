# Plan: comment limits, `notes/` and `specs/`, gating lint on both platforms

## Status

COMPLETE on Linux (uncommitted). Final gate green 170s warm: workspace 1178 passed / 0 failed / 10 ignored; doc-lint 27 passed; `doc-lint --root .` empty; same-code vs BASE: only flake.nix differs (# lines in '' strings). Reviewer approved; review items applied. Pending user approval: three CLAUDE.md clarifications (S1 specs/<topic>.md ownership bullet, nit 1a header-on-line-1, nit 1b seven checked prefixes) — technical-writer declined to apply without user approval. macOS items unverified (see notes/macos-followups.md row for 2026-10-24).

Phase D DONE: full Linux gate green in 181s warm (1178 passed, 0 failed, 10 ignored); `doc-lint --root .` empty; same-code vs BASE: 402 SAME, only flake.nix differs (# lines inside '' only); vs HEAD: 22 expected diffs (Phase A Bazel/lint, Phase B strings, flake.nix). New `specs/markdown-stylesheet.md`. 30 mirror notes files / 83 headings. Net −13,175 comment lines. Next: reviewer, then technical-writer.

Phase A DONE (coder): doc-lint 26/26 tests, markdown_check_test 10/10, 26 falsifiers each failed then reverted; compiles on rustc 1.85.0 via nix rust-overlay. `doc-lint --root .` baseline: 2397 comment-lines, 5 comment-width, 1 doc-ref. `markdown_all.sh`: 61 findings (mostly root reports). Next: Phase B.

BASE decision: Phase C compares against a snapshot copy of the tree taken right after Phase B (`scratchpad/base/`), not a commit — nothing is committed on this branch without the user's say-so. New mirror notes use ``Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.`` (branch HEAD; the code the moved comments describe).

Phase B extra scope (orchestrator): fix `crates/wolf-macos/README.md:48` "advisory"; make `markdown_all.sh` clean (fix the findings in moved reports and the 3 pre-existing ones: `crates/wolf-gnome/src/README.md:565`, `:880` entry-budget, `tools/wolf-pointer-inject/README.md:91` allcaps).

Phase B DONE (coder): moves + SHA headers, links rewritten, CLAUDE.md rewritten; `doc-lint --root .` = 2397 comment-lines + 5 comment-width only; `markdown_all.sh` and `prettier --check` clean; smoke_headless 73 passed; same-code clean except the permitted string edits (actions.rs:1014/1203, smoke_headless.rs path strings). Left on purpose: two string literals that name moved files without being paths (layout.rs:2912 panic text, smoke_headless.rs:4398 echo). BASE snapshot: `scratchpad/base/` (576 files).

Phase C chunk assignment (final, 11 agents; replaces the §7 table):
- C1: `crates/wolf-gnome/tests/smoke_headless.rs`
- C2: wolf-gnome/src `pane_view.rs`, `state_seam.rs`, `window.rs`, `layout.rs`, `geometry.rs`, `editor_inset.rs`, `shell.rs`, `mouse.rs`, `palette.rs`
- C3: every other file in `crates/wolf-gnome/src/`
- C4: every other `crates/wolf-gnome/tests/*.rs`, `crates/wolf-gnome/Cargo.toml`, root `Cargo.toml`, `flake.nix`, `MODULE.bazel`, root `BUILD.bazel`, `.bazelrc` and other root dotfiles, `bazel/**`, `resources/icons/BUILD.bazel`, `tools/wolf-package-smoke.sh`, `tools/wolf-pointer-inject/main.c`
- C5: `crates/wolf-core/src/**` (incl. css), `crates/wolf-core/BUILD.bazel`, `crates/wolf-core/Cargo.toml`
- C6: `crates/wolf-core/tests/**`, `crates/wolf-core/examples/**`
- C7: `crates/wolf-ffi/src/**`, `crates/wolf-ffi/BUILD.bazel`, `crates/wolf-ffi/Cargo.toml`
- C8: `crates/wolf-ffi/tests/**`
- C9: `crates/wolf-macos/Sources/**`, `crates/wolf-macos/BUILD.bazel`, `crates/wolf-macos/release.bzl`, `crates/wolf-macos/*.sh`
- C10: `crates/wolf-macos/Tests/**`
- C11: `runtime/nvim/init.lua`, `runtime/nvim/BUILD.bazel`

Known test gap (accepted, not pursued): no fixture for a Markdown link to a directory or a `/`-rooted link.

### Test-writer API choices (binding for the coder)

- `missing_coverage(package_dirs: &[&str], lint_build_text: &str) -> Vec<String>`, returns full missing labels (`//crates/b:markdown_srcs`); root package is `""` → `//:comment_srcs`, `//:markdown_srcs`.
- `Language` derives `Debug, Clone, Copy, PartialEq`; `Rule::id(&self)`.
- `run(args)`: args exclude argv[0]; findings to stdout one per line; clean run prints nothing to stdout; summaries/errors to stderr.
- `check_root` paths are root-relative with `/`; `notes-orphan` at line 1; `notes-unlinked` at the heading's line. `heading_slug` takes heading text without leading `#`s.
- Sidecar matching is a multiset. Some repo fixtures (space-named dir, `node_modules/`, `generated/`, symlinks) are created at run time in a scratch copy. Trailing-whitespace/CRLF cases are built in code.
- `tools/lint/markdown_check_test.sh` needs `chmod +x`. Run `cargo fmt` on the two test files and `prettier --write tools/doc-lint/tests/README.md` once the crate exists.

BASE for Phase C is recorded after Phase B lands (see "Orchestrator decisions"). Pre-change HEAD: `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Request (from the user)

- Linter rules that enforce comment limits: no more than two lines, each line in a comment no longer than 100 characters. Anything that cannot fit goes in a Markdown file, and the comment links to it — usually in `notes/`.
- `notes/` at the repo root: not necessarily authoritative, the repo's committed memory. `specs/` at the repo root: authoritative. `CLAUDE.md`, `README.md`, `build.md` stay at the root.
- Each `notes/` file carries the commit SHA at which it was last updated and known correct. When a note is found wrong, update it. `specs/` deviations are bugs in the spec or the code.
- A pass over the repo's comments, moving long comments out.

## User decisions (binding)

1. Doc comments are exempt (Rust `///`, `//!`, `/** */`, `/*! */`; Swift `///`, `/** */`). Plain `//`, `#`, `--`, `/* */` are in scope.
2. Cut first, then notes: apply CLAUDE.md's two-question test; delete what fails, shorten what fits in two lines, move only the remainder to `notes/`, one notes file per source file mirroring its path, one heading per moved comment.
3. `specs/`: `architecture.md`, `command-lifecycle.md`. `notes/`: every other root `.md` except CLAUDE.md, README.md, build.md. Per-crate READMEs and source-dir READMEs stay.
4. Each crate's `DECISIONS.md` moves to `notes/<crate-name>-decisions.md`.
5. The comment linter gates. The `//tools/lint` checks on macOS stop being advisory and gate too.

## Orchestrator decisions

- Accept architect judgement calls J1–J8 as written below.
- J9: rewrite only the stale paths in `.claude/agents/*.md` (reviewer.md, coder.md) to point at the moved files. No other change to `.claude/`. Report this to the user.
- Section 8.2 "unit tests in `#[cfg(test)]` in src/" is changed: the test-writer puts those cases in `tools/doc-lint/tests/api.rs` against the public API (`language_for`, slug function — make it `pub fn heading_slug(&str) -> String` in `markdown.rs` re-exported from lib, `missing_coverage`). Wire it as a second `rust_test` target `doc_lint_api_test` in `tools/doc-lint/BUILD.bazel`. The `doc_lint_unit_test` target is still created (crate tests), even if empty.
- Host is Linux: no Bazel, no swift-format, no Swift. Every macOS-only item in §5 is reported to the user as unverified.

## Measured scale (orchestrator scan)

Non-doc comment blocks over 2 lines: Rust ~1,685, Swift ~330, BUILD.bazel 141, .bzl 35, .sh 84, .lua 104, .nix 42, .toml 8, .bazelrc 2, .tpl 2 — about 2,400. `crates/wolf-gnome/tests/smoke_headless.rs` is 38,835 lines.

---

## 1. Summary

Add `doc-lint`, a std-only Rust tool in its own Cargo workspace at `tools/doc-lint/`. It enforces the comment limit on non-doc comments (≤2 lines, ≤100 chars each), checks root-relative Markdown paths written in comments (including `#anchor`), checks notes-mirror conventions, and — Linux root mode only — checks relative links in Markdown. It also has `strip` and `same-code` subcommands that prove an edit touched only comments.

`markdown_check.sh` gains a `notes-header` rule, a `notes/` exemption from the changelog rule, and a test script. Root reports move to `notes/` and `specs/`; each crate's `DECISIONS.md` becomes `notes/<crate>-decisions.md`; CLAUDE.md is rewritten. The `//tools/lint` check targets lose `manual` and clippy `-Dwarnings` becomes default, so they gate `bazel test //...`. ~2,400 over-limit blocks are migrated in parallel chunks.

## Judgement calls (accepted)

- **J1. Rust, not bash/awk.** Comment detection needs per-language string lexing (raw strings, Swift interpolation, Nix `''`, Lua long brackets, shell `$#`/`${x#y}`, heredocs). Rust gives fixture tests under `cargo test` and `rust_test`, and the same lexer powers `same-code`.
- **J2. Standalone crate.** `tools/doc-lint/Cargo.toml` has its own empty `[workspace]`, own `Cargo.lock`, no dependencies — so the root `Cargo.lock` / `crate_universe` repin (macOS-only) is untouched, and Bazel uses plain `rust_library`/`rust_binary`/`rust_test`. Must compile on rustc 1.85.0, edition 2021, no let-chains. Gate needs explicit `--manifest-path` commands.
- **J3. Width measured from the comment marker to end of line**, excluding indentation and preceding code, in Unicode scalar values, trailing whitespace removed.
- **J4. Lua `---` is not exempt.** C `/** */` and Nix `/** */` are not exempt.
- **J5. No suppression mechanism.** Structural exemptions only: excluded paths; and a comment line whose body (after marker) is a single whitespace-free token is exempt from the width rule only (URLs, long paths); it still counts toward 2 lines.
- **J6. Embedded-language comments are not linted** (shell inside Nix `''`, scripts inside Rust strings). The migration still applies editorial rules to embedded `#` comments in `flake.nix` (`shellHook`, `postInstall`, `preFixup`) and `markdown_check.sh`'s awk program, never inside heredoc bodies. Embedded scripts in Rust test strings (`smoke_headless.rs`) are data — not touched.
- **J7. Markdown links are checked in the Linux root mode only** (Bazel runfiles lack arbitrary targets).
- **J8. `notes/` is exempt from the `changelog` rule only.** Every other markdown rule applies to `notes/`; existing findings in moved reports must be fixed.

---

## 2. Files to create

### `tools/doc-lint/`

- `Cargo.toml` — package `doc-lint`, edition 2021, `publish = false`, empty `[workspace]`, `[lib] name = "doc_lint"`, `[[bin]] name = "doc-lint"`, no deps.
- `Cargo.lock` — generated by first `cargo build`, committed.
- `src/lib.rs` — public API:
  - `MAX_COMMENT_LINES: usize = 2`, `MAX_COMMENT_WIDTH: usize = 100`. No env overrides.
  - `enum Rule` with `id() -> &'static str`: `comment-lines`, `comment-width`, `doc-ref`, `doc-anchor`, `notes-orphan`, `notes-unlinked`, `md-link`, `md-anchor`, `bazel-coverage`.
  - `struct Finding { path: String, line: usize, rule: Rule, message: String }`, Display `path:line: [rule] message`.
  - `enum Language { Rust, Swift, C, Css, Lua, Shell, Starlark, Nix, Toml, HashLine }`
  - `fn language_for(path: &Path) -> Option<Language>`
  - `fn lint_source(display_path: &str, text: &str, lang: Language) -> Vec<Finding>` — comment rules only.
  - `fn check_files(cwd: &Path, files: &[PathBuf], cfg: &WalkConfig) -> Vec<Finding>` — comment rules + `doc-ref`/`doc-anchor` (Bazel mode).
  - `fn check_root(root: &Path, cfg: &WalkConfig) -> Vec<Finding>` — walk + every rule (Linux mode).
  - `fn strip_comments(text: &str, lang: Language) -> String`
  - `pub fn heading_slug(&str) -> String` (in `markdown.rs`, re-exported), `pub fn missing_coverage(package_dirs, lint_build_text) -> Vec<String>` (in `coverage.rs`, re-exported).
  - `struct WalkConfig { skip_dir_names, skip_prefixes }`, `WalkConfig::repo_default()`.
  - `fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32`. Exit 0 clean, 1 findings, 2 usage/I-O error. Findings sorted by path, then line.
- `src/main.rs` — exits with `doc_lint::run(...)`.
- `src/lexer/` — `mod.rs` plus `rust.rs`, `swift.rs`, `c_like.rs` (C, CSS), `lua.rs`, `shell.rs`, `starlark.rs`, `nix.rs`, `toml.rs`, `hashline.rs`. `lex(text, lang) -> Vec<Comment>` with `Comment { kind: Line|Block, family, doc: bool, trailing: bool, start_line, end_line, byte_span, lines: Vec<CommentLine { line_no, from_marker: String, body: String }> }`.
- `src/rules.rs` — block assembly + limit rules (§4.1). `src/refs.rs` — doc-path refs. `src/markdown.rs` — slugs, links, anchors. `src/notes.rs` — mirrors, `notes-orphan`, `notes-unlinked`. `src/coverage.rs` — `bazel-coverage`. `src/walk.rs` — never follows symlinks. `src/strip.rs` — `strip` and `same-code`.
- `tests/fixtures.rs`, `tests/api.rs`, `tests/fixtures/**`, `tests/README.md` (describes the test files, sidecar format, fixture layout).
- `README.md` (outside view: purpose, CLI, rule table with exact wording, language table, exclusions, build/test per platform, link to `src/README.md`). `src/README.md` — one `####` entry per source file.
- `BUILD.bazel`:
  - `rust_library(name = "doc_lint_lib", crate_name = "doc_lint", srcs = glob(["src/**/*.rs"], exclude = ["src/main.rs"]), edition = "2021")`
  - `rust_binary(name = "doc_lint", srcs = ["src/main.rs"], deps = [":doc_lint_lib"])`, visible to `//tools/lint:__pkg__`
  - `rust_test(name = "doc_lint_unit_test", crate = ":doc_lint_lib")`
  - `rust_test(name = "doc_lint_fixtures_test", srcs = ["tests/fixtures.rs"], deps = [":doc_lint_lib"], data = glob(["tests/fixtures/**"]), env = {"DOC_LINT_FIXTURES": "tools/doc-lint/tests/fixtures"})` — sandboxed, not manual
  - `rust_test(name = "doc_lint_api_test", srcs = ["tests/api.rs"], deps = [":doc_lint_lib"])`
  - `comment_srcs(exclude = ["tests/fixtures/**"])`; `filegroup markdown_srcs = glob(["**/*.md"], exclude = ["tests/fixtures/**"])` visible to `//tools/lint:__pkg__`.
- `tests/fixtures.rs` resolves fixtures from env `DOC_LINT_FIXTURES` when set (relative; Bazel test cwd is runfiles root), else `concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures")`.

### `tools/lint/`

- `defs.bzl` — `COMMENT_LINT_PATTERNS` (`**/*.rs`, `**/*.swift`, `**/*.c`, `**/*.h`, `**/*.lua`, `**/*.sh`, `**/*.sh.tpl`, `**/*.bzl`, `**/BUILD.bazel`, `**/BUILD`, `**/*.nix`, `**/*.toml`, `**/*.css`) and macro `comment_srcs(name = "comment_srcs", include = COMMENT_LINT_PATTERNS, exclude = [])` → filegroup `glob(include, exclude = exclude, allow_empty = True)`, `visibility = ["//tools/lint:__pkg__"]`. The list must equal doc-lint's language table; say so in both READMEs.
- `doc_lint_check.sh` — `exec "$1" --files "${@:2}"`.
- `markdown_check_test.sh` — runs `markdown_check.sh` over `testdata/markdown/**`, asserts exit status and finding counts per fixture; locates files relative to `$(dirname "$0")` (works in checkout and runfiles).
- `testdata/markdown/` — fixtures in §8.3.
- `markdown_all.sh` — Linux-only; runs `markdown_check.sh` over `git ls-files "*.md" ":!:CLAUDE.md" ":!:.claude/*" ":!:tools/lint/testdata/*" ":!:tools/doc-lint/tests/fixtures/*"`. Single definition of Linux markdown scope.

---

## 3. Files to modify

### Lint and Bazel (Phase A)

**`tools/lint/BUILD.bazel`**
- Remove `manual` from `:clippy_core`, `:clippy_ffi`, `:clippy`, `:rustfmt_core`, `:rustfmt_ffi`, `:rustfmt`, `:swift_format` (keep `no-sandbox`), `:markdown`, `:check`. Keep `manual` on `:clippy.fix`, `:rustfmt.fix`, `:swift_format.fix`, `:fix`.
- Add `rust_clippy(name = "clippy_doc_lint", testonly = True, deps = ["//tools/doc-lint:doc_lint_lib"])` to `:clippy` data; add `rustfmt_test(name = "rustfmt_doc_lint", targets = ["//tools/doc-lint:doc_lint_lib", "//tools/doc-lint:doc_lint"])`.
- Rename aggregate `markdown_srcs` → `all_markdown_srcs`, listing every `//<pkg>:markdown_srcs` in full-label form, including `//tools/lint:markdown_srcs` (new: `glob(["**/*.md"], exclude = ["testdata/**"])`) and `//tools/doc-lint:markdown_srcs`.
- Add `all_comment_srcs` listing `//<pkg>:comment_srcs` for every package.
- Add `sh_test(name = "comments", srcs = ["doc_lint_check.sh"], args = ["$(rootpath //tools/doc-lint:doc_lint)", "$(rootpaths :all_comment_srcs)"], data = ["//tools/doc-lint:doc_lint", ":all_comment_srcs", ":all_markdown_srcs"])`.
- Add `sh_test(name = "markdown_check_test", srcs = ["markdown_check_test.sh"], data = ["markdown_check.sh"] + glob(["testdata/**"]))`.
- Add `:comments` and `:markdown_check_test` to `:check`. Call `comment_srcs()` for this package. Header comment → ≤2 lines linking `tools/lint/README.md`.

**`tools/lint/markdown_check.sh`**
- New rule `notes-header` for paths matching `(^|/)notes/`: the first line that is neither blank nor an ATX heading (`^#{1,6}[ \t]`) must match ``^Last verified at commit `[0-9a-f]{40}`\.$``. Report at that line with message ``first line must be: Last verified at commit `<40-hex sha>`.``; if none, at line 1. Flush per file in `FNR == 1` and `END` using a recorded filename (like `close_entry`).
- Skip the `changelog` block when `FILENAME ~ /(^|\/)notes\//`.
- Header comment ≤2 lines + README link; keep awk-internal comments short (J6).

**`.bazelrc`** — replace the `--config=lint` block with `build --@rules_rust//rust/settings:clippy_flags=-Dwarnings` and a ≤2-line comment; delete `build:lint`.

**`.prettierignore`** — `feat-list-workflow.md` → `notes/feat-list-workflow.md`; add `tools/doc-lint/tests/fixtures/` and `tools/lint/testdata/`; shorten comments.

**Root `BUILD.bazel`** — `markdown_srcs = glob(["*.md", "notes/**/*.md", "specs/**/*.md", "crates/wolf-gnome/**/*.md", "tools/*.md", "tools/wolf-pointer-inject/**/*.md", "resources/*.md"], exclude = ["CLAUDE.md"], allow_empty = True)`. Add `comment_srcs(include = [...])` with top-level patterns only (`BUILD.bazel`, `MODULE.bazel`, `*.toml`, `*.nix`, `.bazelrc`, `.bazelignore`, `.gitignore`, `.prettierignore`, `.envrc`) plus `crates/wolf-gnome/**/<each pattern>` and `tools/**/<each pattern>`. Never a root `**/` pattern. Rewrite the comment.

**Per-package `BUILD.bazel`** — `crates/wolf-core`, `crates/wolf-ffi` (exclude `generated/**`), `crates/wolf-macos`, `runtime/nvim`, `resources/icons`, `bazel/{ia-writer-mono,jetbrains-mono,lighthouse,nvim,nvim-treesitter,nvim-treesitter-textobjects,orlando,rg,treesitter-parsers}`: load `//tools/lint:defs.bzl`, call `comment_srcs()`, and make `markdown_srcs = glob(["**/*.md"], allow_empty = True)` (drops `DECISIONS.md`).

**`crates/wolf-macos/BUILD.bazel`** lines ~228–234: the comment claiming `//tools/lint` targets are `manual` "for the same reason" becomes false — rewrite.

### Docs (Phase A)

- `tools/lint/README.md` — replace "Policy: advisory, not gating" (checks gate under `bazel test //...`; fix targets stay `manual`); add `:comments`, `:markdown_check_test`, `:clippy_doc_lint`, `:rustfmt_doc_lint` rows; add `notes-header` and the `notes/` changelog exemption to the `:markdown` table; remove the `--config=lint` caveat; markdown scope becomes every `.md` except `CLAUDE.md`, `.claude/`, fixtures; add new files to "Files".
- `tools/README.md` — drop "advisory (non-gating)" from `lint/`; add a `doc-lint/` bullet.
- `build.md` — line ~152: both platforms gate; the gate block and order paragraph (§5); line ~310: only `WolfRelease`, `InstallWolf` and the `//tools/lint` fix targets are `manual`; rewrite "Linting and formatting" (~411–439): checks gate on both platforms, `--config=lint` gone, doc-lint pointer, `markdown_all.sh`, scope = every `.md` except `CLAUDE.md` and fixtures.

---

## 4. doc-lint rule definitions

### 4.1 Comments

| Language | Files | Line marker | Block marker | Exempt (doc) |
|---|---|---|---|---|
| Rust | `*.rs` | `//` | `/* */` nesting | `///` (not `////`), `//!`, `/** */` (not `/**/`, not `/***`), `/*! */` |
| Swift | `*.swift` | `//` | `/* */` nesting | `///`, `/** */` |
| C | `*.c`, `*.h` | `//` | `/* */` | none |
| CSS | `*.css` | — | `/* */` | none |
| Lua | `*.lua` | `--` incl. `---` | `--[[ ]]`, `--[==[ ]==]` | none |
| Shell | `*.sh`, `*.sh.tpl`, `.envrc` | `#` at word start | — | none |
| Starlark | `BUILD.bazel`, `BUILD`, `*.bzl`, `MODULE.bazel`, `WORKSPACE*` | `#` | — | none |
| Nix | `*.nix` | `#` | `/* */` | none |
| TOML | `*.toml` | `#` | — | none |
| HashLine | `.bazelrc`, `.bazelignore`, `.gitignore`, `.prettierignore` | line whose first non-space char is `#` | — | none |

Out of scope: XML/HTML comments, Markdown, JSON, lock files, docstrings, embedded-language comments.

Lexer must handle:
- Rust: `"…"` with escapes; `r"…"`, `r#"…"#` (any `#` count), `b`/`c` prefixes at token start; `r#ident` is not a string; char/byte literal vs lifetime/label (after `'`: `\` → char; `'` two chars later → char; else lifetime); nested block comments.
- Swift: `"…"` with recursive `\( … )`; `"""…"""`; raw `#"…"#`, `#"""…"""#` with `\#(`; nested blocks; `#letter` is a directive (code).
- C/CSS: strings and char literals with escapes; blocks don't nest.
- Lua: `"`/`'` with escapes, `[[…]]`, `[=*[…]=*]`, long comments.
- Shell: `'…'`; `"…"` with `\`, nested `$( … )` and backticks; `$'…'`; `#` starts a comment only at line start or after whitespace or one of `;&|()<>`; skip heredoc bodies `<<WORD`, `<<-WORD` (tab-stripped terminator), `<<'WORD'`, `<<"WORD"`, not `<<<`; `#!` on line 1 is not a comment.
- Starlark: `'`, `"`, `'''`, `"""`, `r`/`b` prefixes.
- Nix: `"…"` with escapes and recursive `${ … }`; `''…''` with `'''`, `''$`, `''\x`, `${ … }`.
- TOML: `"…"`, `'…'`, `"""…"""`, `'''…'''`.
- All: strip `\r`, read lossily as UTF-8.

Block assembly: a full-line comment has only whitespace before the marker. Consecutive full-line line comments on consecutive physical lines with the same family and doc-ness form one block (indentation ignored; Rust `//`, `///`, `//!` are distinct families). A block-comment token is its own block. A trailing comment is its own one-line block; a following full-line comment never joins it.

**`comment-lines`**: for each non-doc block count physical lines; for block comments, opener-only/closer-only lines (`/*`, `*/`, `--[[`, `]]`) do not count; blank comment lines (`//`, ` *`) do count. If > 2, report at the block's first line: `comment is N lines (max 2): cut it, shorten it, or move the rest to notes/<path>.md under a heading and link it` (`<path>` = file's repo-relative path).

**`comment-width`**: per non-doc line, measure `from_marker` (opener line from the first marker char; block continuation from first non-whitespace char), trailing whitespace trimmed, `chars().count()`. If > 100: `comment line is N chars (max 100), measured from the comment marker`. Skip a line whose body (text after marker, optional following space, and for block continuations an optional leading `*`) has no whitespace.

### 4.2 References, notes, links

**`doc-ref` / `doc-anchor`** — all comments, doc included, both modes. A reference token starts at a boundary (line start, whitespace, or one of `` ` ' " ( [ < ``), begins with `notes/`, `specs/`, `crates/`, `tools/`, `runtime/`, `bazel/`, `resources/`, continues over `[A-Za-z0-9_.@+/-]` backtracked to end in `.md`, optional `#[A-Za-z0-9_-]+`. Tokens without `.md` (placeholders like `notes/<crate>-decisions.md`, globs) are ignored; refs into a skip prefix are ignored. Resolution is relative to repo root (cwd in `--files` mode). Missing file: `doc-ref`, `<ref> does not exist`. Missing anchor: `doc-anchor`, `<file> has no heading with anchor #x`.

Slug (GitHub): ATX headings outside fences; lowercase; drop every char that is not Unicode alphanumeric, space, `-` or `_` (backticks and dots dropped); spaces → `-`; duplicates get `-1`, `-2`, ….

Mirror: `notes/<P>.md` where `language_for(P)` is `Some`. Root mode only:
- `notes-orphan`: mirror whose `P` does not exist — `notes/<P>.md mirrors <P>, which does not exist`.
- `notes-unlinked`: every `##` heading of a mirror must be the anchor of at least one reference in a comment of `P` — `heading "<text>" (#slug) is not linked from any comment in <P>`.

**`md-link` / `md-anchor`** — root mode only, every walked `.md` incl. CLAUDE.md. Inline links/images `[x](target)`, `<target>` forms, reference definitions `^\s*\[id\]:\s*target`, outside fences and inline code. Ignore `http:`, `https:`, `mailto:`. Strip `"title"`, percent-decode. `#frag` alone checks the same file. Else resolve relative to the file's dir; must exist as file or dir; `.md` + fragment must match a slug.

**`bazel-coverage`** — root mode only, only if `tools/lint/BUILD.bazel` exists. For every walked dir with `BUILD.bazel`, `tools/lint/BUILD.bazel` must contain both `//<dir>:comment_srcs` and `//<dir>:markdown_srcs` (`//:…` for root).

Walk (`repo_default`): never follow symlinks; skip dir names `.git`, `target`, `.direnv`, `.worktrees`, `.claude`, `node_modules`, `.wolf`, `bazel-*`, `*.xcodeproj`; skip prefixes `crates/wolf-ffi/generated/`, `tools/doc-lint/tests/fixtures/`, `tools/lint/testdata/`.

### 4.3 CLI

- `doc-lint --root <dir>` — every rule.
- `doc-lint --files <paths…>` — comment rules + `doc-ref`/`doc-anchor`, relative to cwd (Bazel).
- `doc-lint strip <file>` — code with every comment (doc too) removed, trailing whitespace trimmed, emptied lines dropped, strings verbatim.
- `doc-lint same-code <old> <new>` — exit 0 if strips equal, else 1 with the first differing line. Language from `<new>`'s name.

---

## 5. Gating

macOS: check targets lose `manual` (so `bazel test //...` runs clippy, rustfmt, swift-format, markdown, comments); `-Dwarnings` default in `.bazelrc`. Fix targets keep `manual`; `:swift_format` keeps `no-sandbox`.

Linux gate in `build.md`, run through `nix develop --command bash -c '…'` with `set -euo pipefail`, fast to slow:

```
cargo fmt --all -- --check
cargo fmt --manifest-path tools/doc-lint/Cargo.toml -- --check
prettier --check "**/*.md"
./tools/lint/markdown_all.sh
./tools/lint/markdown_check_test.sh
cargo clippy --manifest-path tools/doc-lint/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path tools/doc-lint/Cargo.toml
cargo run --quiet --manifest-path tools/doc-lint/Cargo.toml -- --root .
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
```

`markdown_all.sh` needs host `git` (the non-pure devShell inherits host `PATH`; say so). There is no flake `checks` output.

Verifiable on Linux: every doc-lint rule over the whole tree including Swift; markdown, prettier, rustfmt, clippy `-D warnings`; all tests; `same-code` proving Swift/Bazel edits are comment-only.

Not verifiable on Linux (report to user and add a `notes/macos-followups.md` row):
- swift-format `lint --strict` findings (unknown count; gating may turn `bazel test //...` red).
- rules_rust 1.85.0 clippy/rustfmt (version skew vs Linux).
- doc-lint compiling on rustc 1.85.
- Every edited `BUILD.bazel`/`.bzl` parses, `defs.bzl` loads.
- Root glob with dotfile names matches.
- `rust_test` `env` resolving the fixture dir in runfiles; `$(rootpath //tools/doc-lint:doc_lint)`.
- Whether comment-only `Cargo.toml` edits trigger a `crate_universe` digest mismatch (fix: `CARGO_BAZEL_REPIN=1 bazel build //crates/wolf-core:wolf_core`).
- Swift still compiles (mitigated by `same-code`).
- `bazel test //...` duration increase.

---

## 6. File moves and link rewrites (Phase B)

Use `git mv`.

- `architecture.md` → `specs/architecture.md`; `command-lifecycle.md` → `specs/command-lifecycle.md`.
- To `notes/` keeping names: `config-decisions-report.md`, `completions-decision-report.md`, `doc-refactor-misc-recommendations.md`, `docs-refactor-plan.md`, `feat-list.md`, `feat-list-workflow.md`, `macos-followups.md`, `orchestration-recommendations.md`, `rpc-timeout-plan.md`, `treesitter-decision-report.md`.
- `crates/wolf-{core,ffi,gnome,macos}/DECISIONS.md` → `notes/wolf-{core,ffi,gnome,macos}-decisions.md`.

SHA header for moved files: insert ``Last verified at commit `<sha>`.`` after the H1 and a blank line (for `feat-list-workflow.md`, which has no H1, as line 1). `<sha>` = `git log -1 --format=%H -- <old path>`.

Rewrite every relative link inside the moved files (depth changes).

Inbound references: `grep -rnE --exclude-dir={.git,target,.direnv,.worktrees,node_modules} '(architecture|command-lifecycle|config-decisions-report|completions-decision-report|doc-refactor-misc-recommendations|docs-refactor-plan|feat-list-workflow|feat-list|macos-followups|orchestration-recommendations|rpc-timeout-plan|treesitter-decision-report|DECISIONS)\.md' .` — baseline 267 hits in 74 files, plus `.claude/agents` (reviewer.md ~18, 20, 57; coder.md ~62). Root-relative paths in comments; file-relative paths in Markdown. Includes bare "`DECISIONS.md`" mentions in comments (→ `notes/<crate>-decisions.md`), `flake.nix` (2), `.prettierignore`, root `BUILD.bazel`, rustdoc links in `vendored_pins.rs`, `packaged_layout.rs`, `release_profile.rs`.

The one permitted non-comment code edit: `DECISIONS.md` paths inside Rust panic-message string literals (e.g. `smoke_headless.rs` ~16145, 16631, 17885, 35915). Record in the report.

**CLAUDE.md rewrite** (keep every style rule; change only):
- Opening: the source of truth becomes `specs/`, `README.md`, `build.md` and every per-directory `README.md`; a difference with the code is a bug in one of the two and must be addressed. The per-directory README rule stays; `notes/` mirror directories are not source directories. "Start with the documentation" gains: then `notes/`, treated as leads to verify against the code.
- New `## specs/ and notes/`: `specs/` authoritative (deviation = bug in spec or code; decide which and fix). `notes/` the repo's committed memory, not authoritative; verify before relying; rewrite a wrong note in place. The exact header line. The SHA is the commit whose tree you checked the file against (`git rev-parse HEAD` at verification), never the commit that adds the edit. Bump only after re-checking the whole file. Moved files keep the SHA of the last commit that touched them. Layout: `notes/<crate>-decisions.md`; `notes/<repo-relative source path>.md` mirrors with one `##` per moved comment, each linked from its comment; everything else free-form. `notes/` exempt from the changelog check; decisions and mirrors still use present tense.
- Ownership list: `architecture.md` → `specs/architecture.md`; `crates/<crate>/DECISIONS.md` → `notes/<crate>-decisions.md` (read before changing the crate; verify like any note); add `notes/<path>.md` bullet (line-level rationale moved from that file's comments; never the file's purpose or API, which the directory README owns).
- New `## Code comments`: non-doc comments ≤2 lines, each ≤100 chars from the marker; the exempt forms; the single-token width exemption; cut/shorten/move order with the two-question test; link form `See notes/<path>.md#<anchor>`; Markdown paths in comments are root-relative and checked; no suppression; never convert a comment to a doc comment to escape the limit; enforced by doc-lint on both platforms (link its README).
- Test-wiring list: add `tools/doc-lint/tests/*.rs` (outside the cargo workspace; `cargo test --manifest-path tools/doc-lint/Cargo.toml` in the Linux gate; `rust_test` targets in `tools/doc-lint/BUILD.bazel`) and `tools/lint/*_test.sh` (an `sh_test` plus a gate line).
- Final `bazel test //tools/lint:markdown` paragraph: not advisory, not `manual`; gates on both platforms; Linux runs `markdown_all.sh`.

Root `README.md`: rewrite links to `specs/` and `notes/macos-followups.md`. Crate READMEs: "Read DECISIONS.md" → link `../../notes/<crate>-decisions.md`.

---

## 7. Migration pass (Phase C)

### Editorial rules (given verbatim to every chunk coder)

1. Scope: only non-doc comment blocks that `doc-lint --files <your files>` reports, plus the embedded `#` comments named in J6 for `flake.nix`. Never edit code, strings or doc comments (except a missed Phase B path fix). Never touch embedded scripts inside Rust strings.
2. Per sentence, apply CLAUDE.md's two-question test. Keep a reason a future agent would otherwise "simplify" away. Delete restatements of code; history (branch names, "was", "previously", amendment narratives, "measured on <host> during <task>" narration — keep the measured fact if it is the reason); emphasis capitals; text duplicating a README/spec/decisions file (link instead); commented-out code. Collapse three-line section banners to one line or delete.
3. If what survives fits in 2 lines of ≤100 chars, rewrite in place in the file's existing comment style (never introduce `/* */` where the file uses line comments).
4. Otherwise write ≤2 lines in the code's voice: the constraint first, then `See notes/<path>.md#<anchor>` on its second line. Move only the surviving reasoning into `notes/<repo-relative path>.md` as present-tense paragraphs (≤4 sentences, ~600 chars, no hard wraps), passing `markdown_check.sh` (no all-caps; `##` headings, never `####`).
5. Notes file shape: H1 `` # `<path>` ``, blank line, ``Last verified at commit `<BASE sha>`.``, then one `##` heading per moved comment (2–6 plain words, no backticks/links/punctuation besides hyphens, unique in file). Several comments may link one heading; every heading must be linked from its own source file. Run `prettier --write` on your notes files.
6. Never convert `//` to `///`/`//!`, or `#` to a docstring.
7. Cross-references: before moving a comment, grep for references to it (e.g. `build.md:76` → `[profile.release]` comment in `Cargo.toml`; `vendored_pins.rs`/`release_profile.rs` → `flake.nix`/`Cargo.toml` formatting comments; `flake.nix` "see the NOTE above"). Fix references inside your own chunk's files; report references in any README, spec, decisions file, `build.md` or other chunk to the orchestrator. Chunk coders edit no shared docs.
8. No notes for dotfiles (`.bazelrc`, `.gitignore`, `.prettierignore`); shorten in place.
9. Keep a two-line form of load-bearing comments that guard a scanner contract: `wolfVendoredPins` formatting, `Cargo.toml` "one key per line, no inline comment", `SAFETY` justifications.
10. `smoke_headless.rs` reads its own source (`include_str!` ~11039, ~11842; needles `CAGE_SPAWN_NEEDLE`, `is_launch_line`). Do not write needle text (a cage spawn or a `"$WOLF_BIN" … &` line) into comments.
11. Concurrency: do not run `cargo test`; do not commit; do not run `cargo fmt` without `--check`; edit only your chunk's files and their mirrors.

### Chunk verification (each coder)

After the orchestrator has one coder build `cargo build --manifest-path tools/doc-lint/Cargo.toml`:
- `tools/doc-lint/target/debug/doc-lint --files <chunk files>` → clean
- every changed file: `git show BASE:f > <scratch>/old`; `doc-lint same-code <scratch>/old f` exits 0 (`flake.nix` exception: confirm only `#` lines inside `''` outside heredocs differ)
- `cargo fmt --all -- --check`; `./tools/lint/markdown_check.sh <notes>`; `prettier --check <notes>`

### Chunks (by directory; mirrors disjoint)

| Chunk | Files | ~Blocks |
|---|---|---|
| C1 | `crates/wolf-gnome/tests/smoke_headless.rs` alone | ~480 |
| C2 | wolf-gnome/src: `pane_view`, `state_seam`, `window`, `layout`, `geometry`, `editor_inset`, `shell`, `mouse`, `palette` | ~250 |
| C3 | rest of wolf-gnome/src | ~240 |
| C4 | other wolf-gnome tests, `crates/wolf-gnome/Cargo.toml`, root `Cargo.toml`, `flake.nix`, `tools/wolf-package-smoke.sh`, `tools/wolf-pointer-inject/main.c` | ~110 |
| C5 | wolf-core/src, `src/markdown/*.css`, wolf-core `BUILD.bazel`, `Cargo.toml` | ~190 |
| C6 | wolf-core tests and examples | ~260 |
| C7a | wolf-ffi `src/lib.rs`, `src/bin/`, `BUILD.bazel`, `Cargo.toml` | ~125 |
| C7b | wolf-ffi tests | ~210 |
| C8 | wolf-macos Sources, `BUILD.bazel`, `release.bzl`, `*.sh` | ~180 |
| C9 | wolf-macos Tests | ~185 |
| C10 | `runtime/nvim/init.lua`, `runtime/nvim/BUILD.bazel` | ~104 |
| C11 | `bazel/**`, `MODULE.bazel`, root `BUILD.bazel`, `resources/icons/BUILD.bazel`, root dotfiles | ~90 |

`tools/lint/*` and `tools/doc-lint` are Phase A.

---

## 8. Test plan

Every test names the smallest source edit that should make it fail; the coder applies that edit to prove the test can fail.

### 8.1 Fixture tests (`tools/doc-lint/tests/fixtures.rs`)

For every fixture file with a known language, read a `<file>.expected` sidecar (lines `LINE RULE`; empty = no findings), run `lint_source`, assert the set of `(line, rule)` pairs equals it exactly. Runs via `cargo test --manifest-path tools/doc-lint/Cargo.toml` and `//tools/doc-lint:doc_lint_fixtures_test`.

Never name a fixture `BUILD.bazel`, `BUILD`, `MODULE.bazel`, `Cargo.toml`, `flake.nix`, `.envrc`, `.gitignore`, or a dir `target` — test those name mappings in `tests/api.rs`. Layout: `tests/fixtures/lang/<lang>/{pass,fail}.<ext>` (more files allowed), `tests/fixtures/repo/...`, `tests/fixtures/same_code/...`.

| Case | Falsifying edit |
|---|---|
| Rust: 2-line `//` passes, 3-line `//` fails `comment-lines` | `MAX_COMMENT_LINES = 3` |
| Rust: exactly 100 chars passes, 101 fails `comment-width` | `>` → `>=` |
| Rust: deeply indented 100-char comment passes | measure from column 0 |
| Rust: 100 em-dashes pass | `.len()` vs `.chars().count()` |
| Rust: trailing spaces after 100 chars pass | no trim |
| Rust: 10-line `///` with 150-char lines passes; same for `//!`, `/** */`, `/*! */` | drop `///` from doc set |
| Rust: 3-line `////` fails | treat `////` as doc |
| Rust: `/**/` followed by 3 lines is not doc | treat `/**/` as doc |
| Rust: `/*\n a\n b\n*/` passes | count delimiter-only lines |
| Rust: nested `/* /* */ */` with 2 text lines passes; following `//` not swallowed | non-nesting lexing |
| Rust: 5 lines of `"// x"` inside a string and inside `r#"…"#` → no findings | no string / raw-string handling |
| Rust: `'"'` then trailing `// c`; `'a` then a string containing `//` | no char/lifetime disambiguation |
| Rust: 101-char trailing comment fails `comment-width` | skip trailing comments |
| Rust: trailing comment + 2 following full-line comments passes | join trailing into next block |
| Rust: `///` then three `//` reports only the `//` block's line | merge families |
| Rust: 150-char single-URL line passes; `See <url>` over 100 fails | drop single-token exemption |
| Swift: `"\(f("//"))"`, `"""` with `//` lines, `#"//"#`, nested blocks → no findings | no interpolation/multi-line/raw handling |
| Swift: `///` and `/** */` exempt; 3 `//` lines fail | — |
| C: `/** … */` with 3 text lines fails | add `/**` to C doc set |
| C: `"//"` and `'/'` → no finding | — |
| CSS: 3-line block fails, 2-line passes | — |
| CSS: `content: "/*"` does not open a block | no CSS string handling |
| Lua: 3 `---` lines fail | add `---` to doc set |
| Lua: `--[==[` long comment with 3 text lines fails | — |
| Lua: `[[ -- x ]]` and `"--"` are not comments | no long-string handling |
| Shell: shebang + 2 comments passes | don't skip shebang |
| Shell: `$#`, `${#x}`, `${x#y}`, `a#b`, `"# x"`, `'#'` | drop word-start rule |
| Shell: `$( cmd # c )` is a comment | — |
| Shell: heredoc bodies `<<EOF`, `<<'EOF'`, `<<-EOF` with `#` lines → no finding | no heredoc skip |
| Shell: 3-line `#` block fails | — |
| `x.sh.tpl` lexed as shell | drop `.sh.tpl` mapping |
| Starlark `.bzl`: `"""` docstrings with `#` lines, `'#'` | no triple-quote handling |
| Nix: `''` strings with `#` lines, `'''`, `''${`; `"${ "#" }"` | no `''`/antiquote handling |
| Nix: `/* */` with 3 lines fails; 3 `#` lines fail | — |
| TOML: `'''#'''`, `"#"`, trailing comment after a key | — |
| HashLine `lang/hashline/.bazelrc`: 3 `#` lines fail; `build --x # y` not a comment | — |
| `notes.txt` skipped | — |

Repo-mode fixtures `tests/fixtures/repo/` (run `check_root`, assert exact findings):
- `src/a.rs`: `// See notes/src/a.rs.md#good-heading` passes; `notes/src/missing.rs.md` fails `doc-ref` (falsify: drop existence check); `#nope` fails `doc-anchor`; doc comment `/// specs/nope.md` fails `doc-ref` (falsify: scan only non-doc); a string literal with `notes/x.md` not checked; `notes/<crate>-decisions.md` placeholder ignored.
- `notes/src/a.rs.md`: `## Good heading`, ``## The `foo()` call`` (anchor `the-foo-call`), two `## Dup` (comments link `#dup-1`; falsify: keep backticks / no dup suffix), and `## Unlinked` fails `notes-unlinked`. Every non-`Unlinked` heading is linked from `src/a.rs`.
- `notes/src/deleted.rs.md` fails `notes-orphan`; `notes/free-form.md` no orphan check.
- `docs/x.md`: `../src/a.rs` passes; `missing.md` fails `md-link`; `../notes/src/a.rs.md#good-heading` passes; `#nope` fails `md-anchor`; `https://…` ignored; links in a fence and in inline code ignored; `%20` path to `dir with space/f.md` passes; ref def `[r]: missing2.md` fails.
- `node_modules/x.rs` and `crates/wolf-ffi/generated/g.swift` with 3-line comments → no findings.
- No `tools/lint/BUILD.bazel` in the fixture, so `bazel-coverage` is skipped; `bazel-coverage` is covered via `missing_coverage` in `tests/api.rs`.

`same_code/` pairs: `old.rs` vs `comment_only.rs` equal (falsify: keep blank lines); vs `code_changed.rs` (one token) different; vs `string_changed.rs` (edit inside a string containing `//`) different (falsify: regex strip); `old.nix` vs `string_comment.nix` (a `#` line inside `''` edited) different.

CLI `run()`: clean → 0; one finding → 1 with exact output `path:3: [comment-lines] comment is 3 lines (max 2): …`; nonexistent file → 2; no args → 2.

### 8.2 API tests (`tools/doc-lint/tests/api.rs`)

| Behaviour | Falsifying edit |
|---|---|
| `language_for`: `BUILD.bazel`, `BUILD`, `MODULE.bazel` → Starlark | drop exact-name matches |
| `language_for`: `.envrc` → Shell; `.gitignore`, `.prettierignore`, `.bazelignore`, `.bazelrc` → HashLine; `x.sh.tpl` → Shell; `Cargo.lock`, `fonts.conf`, `x.json` → None | — |
| `heading_slug`: punctuation, Unicode letters, underscores kept, backticks/dots dropped | — |
| `missing_coverage` with one package unlisted → reported | always return empty |
| root label form `//:comment_srcs` | — |

### 8.3 `markdown_check_test.sh` (fixtures `tools/lint/testdata/markdown/`)

Runs as `//tools/lint:markdown_check_test` and as a gate line.

| Fixture | Expected | Falsifying edit |
|---|---|---|
| `notes/good.md` (H1, SHA line, prose with "now") | 0 findings | remove changelog exemption |
| `notes/missing.md` | 1 `[notes-header]` | drop rule |
| `notes/short-sha.md` (7 hex) | 1 `[notes-header]` | `{7,40}` |
| `notes/no-h1.md` (SHA on line 1) | 0 | — |
| `notes/nested/deep.md` | 0 | — |
| `notes/empty-after-h1.md` | 1 `[notes-header]` at line 1 | no end-of-file flush |
| `other/changelog.md` ("now") | 1 `[changelog]` | exempt every path |
| `other/no-sha.md` | 0 `[notes-header]` | — |

Anchoring the path match as `^notes/` instead of `(^|/)notes/` is also a falsifier.

---

## 9. Implementation order

- **A1.** `tools/doc-lint` crate, tests, fixtures, READMEs. Green on fmt, clippy, test for that manifest.
- **A2.** `markdown_check.sh` rules, `markdown_check_test.sh`, testdata, `markdown_all.sh`.
- **A3.** Bazel: `defs.bzl`, `tools/doc-lint/BUILD.bazel`, every package's `comment_srcs`/`markdown_srcs`, `tools/lint/BUILD.bazel`, `.bazelrc`, `.prettierignore`, the `crates/wolf-macos/BUILD.bazel` comment, migrate comments in every `tools/lint` file.
- **A4.** `tools/lint/README.md`, `tools/README.md`, `build.md` gate and lint sections.
- **B.** Moves, SHA headers, link rewrites, CLAUDE.md, root `README.md`, `.claude/agents` paths. Verify: `doc-lint --root .` (only `comment-*` findings and pre-existing ones may remain), `markdown_all.sh`, `prettier --check`, `cargo build --workspace --all-targets`, `cargo test -p wolf-gnome --test smoke_headless` for the string edits. Then orchestrator records BASE.
- **C.** Parallel chunks C1–C11.
- **D.** Integration: apply reported shared-doc edits; add the `notes/macos-followups.md` row for §5's unverified items; run the full Linux gate once (no concurrent `cargo test`); `same-code` over `git diff --name-only BASE`; `grep -rn 'DECISIONS\.md' .` (excluding `.git`, `target`, `.claude`) expects 0 hits.

## 10–11. Dependencies and lint

No new packages; std only. No `#[allow]` expected — split large lexer functions instead. Edition 2021, rustc 1.85 compatible. Fixtures excluded from prettier, Bazel globs, doc-lint walk, `markdown_all.sh`. Fixture `.rs` files sit under `tests/fixtures/` so cargo test discovery and rustfmt don't reach them. Never introduce `/* */` in Swift.

## Phase D inbox (cross-references reported by chunk coders)

- C7: verify `crates/wolf-ffi/tests/README.md` records the "single `#[test]`" constraint for the `state`, `command_palette`, `multi_window` targets (C7 deleted the BUILD.bazel blocks that said so; `recent_dirs` confirmed). Restore to README if missing.
- C7: `notes/macos-followups.md:58` and `:65` describe wolf-ffi BUILD.bazel comment blocks that are deleted (orlando/lighthouse/treesitter target blocks) — update those rows.
- C7: `notes/doc-refactor-misc-recommendations.md:60` — the lib.rs ~396 per-window wake comment is fixed; the `///` doc on `WakeCallback` (~1194) still says per-window (doc comment, out of Phase C scope).
- C11: `crates/wolf-core/tests/nvim_restart.rs:314` cites "init.lua's own note above wolf_setup_buffer_tracking" (cut) — repoint to `runtime/nvim/README.md#decisions`.
- C11: `crates/wolf-gnome/src/preview_link.rs:72`, `:673`, `:1120` cite `runtime/nvim/init.lua:2229` (stale) for the empty-source-path filter — cite by name (`wolf_setup_preview`'s `emit_previews`) instead. `:1120` is in a string literal: leave it unless trivially safe, and report.
- C11: `flake.nix:39` says init.lua and test comments state measured claims per Neovim version — most now live in `runtime/nvim/README.md` and `notes/runtime/nvim/init.lua.md`.
- C11: DOC/CODE DIVERGENCE: `runtime/nvim/README.md` says `wolf_setup_buffer_tracking` reports windows of the current tabpage; code scans `nvim_list_wins()` (every tabpage). Verify by reading code; fix the README (code is the fact).
- C11 FYI (free-form notes, low priority): stale init.lua line numbers in `notes/doc-refactor-misc-recommendations.md:52`, `:92`, `notes/rpc-timeout-plan.md:133` (that also wrongly claims `swapfile = false`).
- C2 FYI: doc comment at `crates/wolf-gnome/src/layout.rs:152` references "the test-contract NOTE above", which does not exist (pre-existing; doc comment). Fix if trivial.
- C1: doc comment `smoke_headless.rs` ~10052 names "'Note (OCR width)' / 'Note (OCR height)' paragraphs above their own consts" — now in `notes/crates/wolf-gnome/tests/smoke_headless.rs.md#split-pane-smokes`; repoint the doc comment. ~15301 panic string "see the cmdline-visibility note above the VSPLIT_* consts" — still resolves via the shortened VSPLIT comment's link; leave the string.
- C5 AUTHORITY DECISION (orchestrator): `crates/wolf-core/src/markdown/README.md:11`, `:64`, `:72` call `base.css`'s header comment "the single source of truth" for the baseline-grid arithmetic. That content is now in `notes/crates/wolf-core/src/markdown/base.css.md`, which is non-authoritative. Decision: content a README declares authoritative goes to `specs/`. Move the authoritative sections (baseline grid, border compensation, inline-code leading, edge rules, no-markup rule — whatever the README treats as the spec) to `specs/markdown-stylesheet.md` (no SHA header; specs are authoritative), repoint the `base.css` comments and the README lines there, and delete the mirror if nothing remains. Keep `notes-unlinked` clean.
- C5: stale references to the old `base.css` header in `crates/wolf-core/tests/markdown_render.rs` `:525` (doc), `:618` (doc), `:953` (comment), `:1201` (panic string, leave); `crates/wolf-gnome/tests/smoke_headless.rs` `:22020`, `:24625` (doc comments) — repoint doc/comments to the new spec location.
- C5: `crates/wolf-core/tests/tabpages.rs:955` (doc) quotes the `redraw.rs` win_close comment — now in `notes/crates/wolf-core/src/redraw.rs.md#orphaned-nav-history`; repoint. `:1052` panic string: leave.
- C5 FYI: `notes/macos-followups.md:37`, `:39` cite `crates/wolf-core/BUILD.bazel` line numbers that shifted; `notes/doc-refactor-misc-recommendations.md:75` describes a deleted liveness.rs NOTE. Stale `//!` in `crates/wolf-core/src/config.rs:8-11` ("per the architect plan §2") and `liveness.rs:1` ("architect plan §1") — doc comments with history; fix in Phase D (doc-comment edits are allowed in D as doc fixes, not limit-driven).
- C9: stale line-number citations (pre-existing, made worse): `crates/wolf-gnome/src/mouse.rs:320` (doc) cites `PaneContainerView.swift:807` (now ~885); `crates/wolf-gnome/src/search_overlay.rs:32` cites `SearchView.swift:364-366` (now ~362-363). Replace line numbers with symbol names. `notes/macos-followups.md:20,29,38,40,55,56` cite shifted line numbers — update or cite by symbol.
- C4: `build.md:76` says per-key reasons "belong to the comment above `[profile.release]`" — now `notes/Cargo.toml.md#release-profile`. `crates/wolf-gnome/tests/release_profile.rs:155` (doc) says rationale lives in the comment block above the profile — repoint to the notes. `:514` string, `:702` doc: leave.
- C4: `smoke_headless.rs:36033`, `:36044` refer to "flake.nix's linuxShell comment" — now `notes/flake.nix.md#webkitgtk-and-libsoup`; repoint if they are comments/doc comments.
- C4 FYI: `bazel/ia-writer-mono/extensions.bzl` docstring line ~41 says update "flake.nix's iaWriterMono" but the pin is in `wolfVendoredPins` (pre-existing).
- C4 note for user: flake.nix `''` comment edits change the devShell and package derivations (one-time re-evaluation/rebuild).

## Review round 1 (reviewer: approved with minor notes, no blockers)

Routing (orchestrator):
- test-writer: S4 — CLI test for `--files` doc-ref resolution against cwd (one resolvable + one missing `notes/…md#anchor`, exactly one `[doc-ref]`; falsifier: resolve against file's parent). Plus fixtures for: shell `$(( x << name ))` is arithmetic, not a heredoc; Lua `--]]` closer-only line does not count toward the limit.
- coder: make those pass; `markdown_all.sh` must skip index entries deleted from the worktree; restore `Cargo.toml:10` reason (scanner is a line reader, an inline comment would be read as part of the value) within 2 lines; optional rewording of `pane_view.rs:601`, `:817`, `:1222-1223` and history voice at `smoke_headless.rs:1003`; confirm exec bits on new `tools/lint/*.sh`; falsifier evidence for new tests.
- technical-writer: S1 (CLAUDE.md `specs/<topic>.md` ownership bullet; workspace-doc test does not decide where a spec lives), S2 (markdown README restates spec — cut to decision + link), S3 (`notes/macos-followups.md` row 68: `bzlTransitiveDigest`/`MODULE.bazel.lock` rewrite after `extensions.bzl` comment edits; row 15 stale `manual` claim), nit 1 (CLAUDE.md header-on-line-1 when no H1; only the seven prefixes are checked), nit 2 (markdown scope mismatch build.md:439 vs tools/lint/README.md:72; host-git note duplicated), nit 3 (clippy also fails `bazel build //...`; wolf-macos README: only the swift-format check target gates), nit 7 (macos-followups: swift-format may count Swift comment width from column 0).
- Not pursued: `heading_slug` combining marks (no such headings in tree).

## Review round 1 — tech-writer done (no shell; prettier/lint not yet run on its files)

Applied: S2, S3, nits 2, 3, 7; fixed 7 doc/code divergences (doc side). Orchestrator decision: apply its drafted CLAUDE.md text for S1 and nit 1 (within the user's request to define specs/notes conventions; flag to user). Pending after the coder's round-1 lexer fixes: `tools/doc-lint/src/README.md` `lexer/shell.rs` entry (arithmetic `<<`), `tools/doc-lint/README.md` delimiter-only list (+`--]]`), `tools/lint/README.md:72` ("and present in the worktree"), `.bazelrc:19` comment (clippy fails `bazel build //...` too).

---

# Follow-up task: remove workflow history from code; fix stale strings

## Request (from the user)

Resolve the two items left as-is: (1) error-message strings that still describe text that moved; (2) the "plan §N" history references remaining in code.

## Measured (orchestrator, current tree, excluding *.md, fixtures, .claude/, binaries)

- `§`: 151 hits; "NOTE to coder"/"the coder"/"coder must": 97; "test-writer"/"architect": 20; "from the plan"/"the plan"/"plan's": 89; `OD#`/`OQ#`: 62; `(D#)`: 4. About 100 files (wolf-core tests 38, wolf-ffi tests 22, wolf-core src 12, wolf-macos Tests 9, wolf-gnome src 8, macos Sources 5, gnome tests 4, ffi src 1, doc-lint tests 1). `resources/icons/*` hits are binary/SVG false positives — skip.
- By kind (narrow regex): ~232 doc comments, ~12 plain comments, ~19 string literals.
- False positives exist: "plan" is a domain term in `crates/wolf-gnome/src/state_seam.rs` (restore plan), `smoke_headless.rs` embedded-shell echo text about restore plans, etc. Legit `§` uses (e.g. section refs into a real spec like `specs/architecture.md` §Output) are fine when they name a real document.
- Stale strings already identified: `crates/wolf-core/tests/tabpages.rs:~1052` (quotes old redraw.rs win_close comment; now substantively wrong), `crates/wolf-core/tests/markdown_render.rs:~1200` (base.css header A/B → specs/markdown-stylesheet.md), `crates/wolf-gnome/src/preview_link.rs:1120` (`init.lua:2229`), `crates/wolf-gnome/tests/smoke_headless.rs:~15301` (VSPLIT cmdline-visibility note), `crates/wolf-gnome/tests/release_profile.rs:514` ("comment above it"), `crates/wolf-gnome/src/layout.rs:2912` ("architecture.md used to document"), `crates/wolf-gnome/tests/smoke_headless.rs:4398` (embedded echo naming macos-followups.md).

## Follow-up plan (architect; approved by orchestrator)

Orchestrator decisions: adopt §8 prevention rule `workflow-ref` (lands after the editorial pass). PRE snapshot of `crates/` taken at `/tmp/claude-1000/-home-jplein-dev-jplein-wolf--worktrees-feat-documentation-checks-and-updates/246ae29a-6a21-4f1c-b4a6-a4b6369bd31d/scratchpad/pre-history/` (361 files). doc-lint built at `tools/doc-lint/target/debug/doc-lint` (DL). No Markdown pass needed (architect grep: READMEs, specs, notes/crates have zero hits; specs/command-lifecycle.md § refs are real).

### F1. Scope
Editorial pass over `crates/` only, plus eight stale message strings. Free-form `notes/*.md` reports stay as they are.

### F2. Classification and editing rules (verbatim for every coder)

**A. Workflow history (must go)** — a token citing something that doesn't resolve to a repo file or a same-file definition (grep the label in the same file and in `**/*.md` excluding `.claude/`):
- Plan citations: `plan §N`, `(plan §3.1)`, `per plan`, `per the plan`, `the plan's …`, `feature/fix/search/appearance/architect plan`, `the design §2.2`, `D2 in the plan`, `§D4`, `§B4`, `§C1`, `§Q1`, `§A`, `§FFI`, `§SLUG`, `§Constraints`, `§"…"`, `DECISION §13.6`, `RESOLVED §11`, `§16 clarification`, `§5.4's predicted red`.
- Decision/question ids: `OD1`–`OD12`, `OQ#`, `FD2/FD3/FD5`, bare `D1`–`D8` (`decision D4`, `(D7)`, `(D3)`), `O2/O3/O5/O6/O11–O17/O32`, `Design S`, `requirement 2`, `requirement 6`, `review note N1`.
- Review/test-plan ids (wolf-macos only): `H1`, `H2`, `L1`, `F-1`, `F-2`, `DEV-2`, `T-5`–`T-8`.
- Milestone labels used temporally: `new in S1`, `In S1 there are no overrides`, `the S1 multigrid foundation`, `the S2 async streaming traversal`, `S3`, `Phase 1`, `Phase 2 pass`, `Phase 0`, `pass 8 REFUTED`, `in v1`.
- Role instructions: `NOTE to coder`, `the coder adds/implements/runs/must`, `the coder's to fill`, `Measured by the coder`, `test-writer`, `## Dependency on coder work …`, and "does not exist yet / will not compile until they land" inside those passages.

**B. Legitimate (stays):** "plan" as a domain value (`RestorePlan`, `TabPlan`, `restore_plans`, `plan_for_window` in `state_seam.rs`; `coordinator.rs:332`; `tab_view.rs:784`; `RestorePlan`/`ProjectPlan`/Phase A/B in `RestoreSeam.swift` and `AppDelegate.swift`; restore-smoke `echo` lines in `smoke_headless.rs` ~6891, 7040–7054). `§` naming a real document section (`CSS 2.1 §10.8` ~22019). File-local names defined in the same file (`T1/T2/W1/W2` in `tabpages.rs`; `H1/H2` heights in `smoke_headless.rs` and `tabpages.rs:816`; numbered assertions, legs (a)–(h), rows; S1 drawer readback consts in `smoke_headless.rs` ~29487–29517; `F1`/`F5` keys). "Falsifying edit:" — keep content, drop " (from the plan)". Unrelated words: `Information Architects Inc.`, `specs/architecture.md`, `init?(coder: NSCoder)`, `-O2`.

**C. Edit actions, in order of preference:**
1. Delete the label only when the sentence stands without it (`(plan §3.1)`, `(OD8)`, `Falsifying edit (from the plan):` → `Falsifying edit:`). Fix leftover punctuation.
2. Replace the id with the decision's name when the sentence needs it (`The whole refocus-rebind design (OD2) rests on this` → `The refocus-rebind design rests on this`). Find names in READMEs/specs/notes; link only to anchors that exist (doc-lint checks `notes/…md#x` in all comments). wolf-macos review ids: pick one 2–5-word phrase per id from context and use it consistently in that file (e.g. H1 → "the source-switch reset", H2 → "the seed-clobber guard", L1 → "the unchanged-offset arm", F-1 → "the clamped-offset arm"); MARK lines drop the `T-5 / H1:` prefix.
3. Rewrite in present tense when the substance is durable: `MEASURED by the coder, 2026-08-04, on …` → `Measured 2026-08-04 on …`; `NOTE to coder: if this suite is ever run on a big-endian target …` → the constraint without the label (`paint_golden.rs:187`, `:1774`, `gdk_key.rs:14`); `because plan §B11's crate-root re-export list does not name it` → `because the crate root does not re-export it` (verify in `lib.rs`); `The plan predicted …` → what the test observes; `The \`measured\` column is the coder's to fill from an actual run` (×12 in smoke_headless.rs) → read the table: values present → "The `measured` column is from an actual red run"; unmeasured → "Rows marked unmeasured have not been run against a captured red".
4. Delete the whole sentence/paragraph/section when it only instructed an implementer or described pre-implementation state: every `## Dependency on coder work (…)` section (`pane_buffers.rs:14–20`, `panes.rs:19–26`, …); `NOTE to coder (D4 view shape): … align them with §D4` (`file_search.rs:20–25`); `The coder runs it FIRST; …` (`tabpages.rs:410–411`); `… do NOT exist yet — the coder adds them per the fix plan §2. This test will not compile until they land` (`wolf-core/src/layout.rs:261–262`); `the NEW pane surface the coder adds per plan §3:` → `the pane surface:` (`panes.rs:6–7` and siblings); `ONE READING THE PLAN LEAVES OPEN, …` → state the chosen reading as a decision (`frame_stats.rs:994`). Before deleting, keep any still-true fact (BUILD wiring, "single #[test]") as one present-tense sentence.

**D. Invariants:** a public item's doc comment still says what the item is (first sentence survives). Re-wrap only paragraphs you edit; lines ≤100 columns (Swift: ≤100 from column 0). Edited plain comments stay ≤2 lines × ≤100 chars from the marker; never `//` → `///`; delete a comment the edit empties. No new intra-doc links; keep `[`…`]` balanced. Never write "now", "no longer", "previously", "used to". In `smoke_headless.rs`, never write the cage-spawn needle or a `"$WOLF_BIN" … &` line (the file reads itself via `include_str!` ~10210, ~11013).

### F3. String literals
Rust/Swift message strings may be edited when the literal only flows into a message (`assert*!`, `panic!`, `expect`, `poll_until` descriptions, `desc`/`case` fields only formatted into messages, XCTest messages). Before editing: (1) `rg -n 'should_panic' crates` — confirm zero or not your text; (2) `rg -n 'include_str!\("[^"]*\.(rs|swift)"\)' crates` — only `smoke_headless.rs` reads itself; your edit must not create/remove text matching `CAGE_SPAWN_NEEDLE`, `is_launch_line`, `PID_CAPTURE_LINE`, `is_shell_command`; (3) trace the binding — if compared, written, sent to nvim, or used as a key, do not edit; report. Swift string edits: only characters between quotes; never touch `\`, `"`, `\(`, `+`; lines ≤100. Embedded shell in `smoke_headless.rs` raw strings: edit only if (a) a stale-ref fix (F4 #6) or a §A id (e.g. `# --- Step 9: O32 leg 1 --` ~29737, ~29755) AND (b) a `#` shell comment or `echo … >&2`. Keep the raw string's physical line count; never touch an `echo` without `>&2`, a heredoc body, or lines with `$WOLF_BIN`, `cage`, `wait_ready`, `hsend`. Domain "plan" echoes stay.

### F4. The eight stale strings
1. `crates/wolf-core/tests/tabpages.rs` ~1052–1053: replace the clause from `` and `crates/wolf-core/src/redraw.rs`'s `win_close` comment (\"…\") turns out to be the accurate one. `` with `` and `notes/crates/wolf-core/src/redraw.rs.md#orphaned-nav-history` and `specs/architecture.md` (core↔front-end contract, tabpage scope), which both state that a background tabpage's window IS in `win_buffers` and `buffers`, are wrong and must be corrected. `` Also :454 `if this fails the coder must STOP and report.` → `if this fails, that design is unsound.`; :452 and :462 `KEYSTONE FAILURE (OD2):` → `KEYSTONE FAILURE (refocus rebind):`. Leave :1057 `If it is red, STOP and report.`
2. `crates/wolf-core/tests/markdown_render.rs:1200`: ``\`base.css\`'s header records the A/B:`` → ``\`specs/markdown-stylesheet.md\` (\"Inline code leading\") records the A/B:``.
3. `crates/wolf-gnome/src/preview_link.rs:1121`: `(init.lua:2229 keeps such a preview from ever becoming a pane)` → ``(`emit_previews` inside `wolf_setup_preview` in `runtime/nvim/init.lua` keeps such a preview from ever becoming a pane)``. Also drop `O12:`/`O13:` prefixes in `desc` strings ~923, ~1159.
4. `crates/wolf-gnome/tests/smoke_headless.rs:15301–15302`: `(see the cmdline-visibility note above the VSPLIT_* consts)` → ``(see the \"Cmdline visibility\" paragraph of `notes/crates/wolf-gnome/tests/smoke_headless.rs.md#split-pane-smokes`)``.
5. `crates/wolf-gnome/tests/release_profile.rs:513–514`: `Restore the block described in the comment above it.` → ``Restore the block; `notes/Cargo.toml.md#release-profile` gives each key and its reason.``
6. `crates/wolf-gnome/tests/smoke_headless.rs:3952` (embedded `echo … >&2`): `macos-followups.md records the Swift side still has.` → `notes/macos-followups.md records the Swift side still has.` Keep it a single `echo "…" >&2`.
7. `crates/wolf-gnome/src/layout.rs:2823`: delete `` — the behaviour `architecture.md` used to document as a limitation``; end at `dragged geometry.`
8. `crates/wolf-gnome/tests/vendored_pins.rs:307–309`: delete `BEFORE the packaging unit lands, the block simply does not exist yet and this failure is expected — add it, in the shape … states. AFTER it lands,`; begin `The binding was renamed or reformatted, in which case …`; fix "Two states look like this" to fit one state.

Other message-only edits from §A: `wolf-gnome/src/palette.rs` 2369, 2674 (D2 → the "type what you see" label rule, as at :2603); `nav_ui.rs:717`; `frame_stats.rs:2054`; `wolf-ffi/tests/cmdline_ui.rs:230`; `wolf-ffi/tests/tabpages.rs:247`; `wolf-core/tests/nav_history_live.rs` 1270, 1452; `smoke_headless.rs` 15884, 33984, 34052; Swift `FrameStatsTests.swift:382` → `"142 wakes / 48 keys -> a non-integer ratio"`, `RestoreSeamTests.swift:120`, H1/H2/L1/F-1 messages in `PreviewScrollSeamTests.swift`.

### F5. Chunks (disjoint; no shared docs edited; cross-refs to orchestrator)
| Chunk | Paths | Stale strings |
|---|---|---|
| K1 | `crates/wolf-core/tests/[a-m]*.rs`, `crates/wolf-core/tests/common/**` | #2 |
| K2 | `crates/wolf-core/tests/[n-z]*.rs` | #1 |
| K3 | `crates/wolf-ffi/**` | — |
| K4 | `crates/wolf-core/src/**`, `crates/wolf-gnome/src/**` | #3, #7 |
| K5 | `crates/wolf-gnome/tests/**` | #4, #5, #6, #8 |
| K6 | `crates/wolf-macos/**` | — |

Finder (set DIRS; run from repo root):
```
PAT='(?i:\bnote to (?:the )?coder\b|\bthe coder\b|\bcoder(?:\x27s)?\s+(?:work|adds|implements|must|runs|should)\b|\bcoder\x27s\b|\bby the coder\b|\btest[- ]writers?\b|\barchitect(?:\x27s)? plan\b|\b(?:feature|fix|search|appearance|design) plan\b|\bper (?:the )?plan\b|\bfrom the plan\b|\bin the plan\b|\bthe plan\b|\bplan\x27s\b|\bplan §|\breview (?:note|finding)s?\b)|§|\b(?:OD|OQ|FD)\d+\b|(?<![-\w])[DO]\d{1,2}\b|\bDECISION\b|\bRESOLVED §|\bDesign [A-Z]\b|\brequirement \d\b|\b(?:new )?in S\d\b|\bS\d (?:multigrid|async|foundation)\b|\bPhase \d\b|\bpass \d+ REFUTED|\bdo(?:es)? not exist yet\b|\bnot compile until\b'
rg -nP -g '!*.md' -g '!*.{svg,png,icns,txt,b64,lock,json}' "$PAT" $DIRS
```
K6 also: `rg -nP '\b[HL]\d\b|\bF-\d\b|\bT-\d\b|\bDEV-\d\b' crates/wolf-macos`. "does not exist yet" has domain hits in `state_seam.rs`, `actions.rs`, `shell.rs`, `PaneContainerView.swift:1324` — leave them.

### F6. Verification (each coder, repo root)
1. `CHANGED=$(diff -rq "$PRE/<dir>" <dir> | awk '/differ$/{print $4}')` per chunk dir.
2. `$DL --files $CHANGED` → no output.
3. Files with no string edit: `$DL same-code "$PRE/$f" "$f"` → exit 0.
4. Files with string edits: `diff <($DL strip "$PRE/$f") <($DL strip "$f")`; every hunk is a planned string line; report `file:line old → new` (embedded-shell lines separately).
5. `cargo fmt --all -- --check`.
6. K5 only: `cargo test -p wolf-gnome --test smoke_headless cage_is_only_spawned_through_the_isolating_helper every_launch_waits_for_readiness_before_its_first_input`.
7. No commits, no `cargo fmt` without `--check`, no other `cargo test`.

Final gate once after all chunks. Residual check: `rg -nP -g '!*.md' -g '!.claude/**' -g '!tools/doc-lint/tests/fixtures/**' -g '!tools/lint/testdata/**' -g '!*.{svg,png,icns,txt,b64,lock,json}' "$PAT" .` — allowed: domain "plan" in `state_seam.rs`, `coordinator.rs`, `tab_view.rs`; restore-smoke echo lines; `CSS 2.1 §10.8`; "does not exist yet" domain lines. wolf-macos id grep empty except `F1`/`F5`.

### F7. Prevention rule `workflow-ref` (doc-lint; after the pass)
- Scope: every comment, doc comments included, every language, both `--files` and `--root`. Strings and Markdown not scanned. One finding per comment line, first match.
- Message: `workflow reference "<matched text>": it cites a planning document that is not in the repo; state the reason in present tense or delete it`.
- Case-insensitive substring match with word boundaries both ends (non-alphanumeric, non-`_`): `note to coder`, `note to the coder`, `the coder`, `test-writer`, `test writer`, `architect plan`, `the architect`, `per plan`, `per the plan`, `from the plan`, `feature plan`, `fix plan`, `plan §`, and tokens `OD<digits>`, `OQ<digits>`.
- Not banned: bare `the plan`, `§`, `D\d`, `O\d`, `H1`/`L1`, `S\d`.
- Patterns live in a `const` table of string literals (doc-lint's own comments must not spell them).
- Docs: `tools/doc-lint/README.md` rule table, `src/README.md` entry. CLAUDE.md mention needs user approval.
- Tests: `tests/fixtures/lang/.../workflow_ref.rs` reported: `/// Falsifying edit (from the plan): x`, `//! per plan §3`, `// NOTE TO CODER: y`, `// see OD2.`, `/* the test-writer adds */`; not reported: `let s = "per plan §3";`, `// COD2 ODE OD restore plan architecture Information Architects`, `// the restore plan's tabs`. `workflow_ref.swift`: `/// OQ3 decides` reported; `init?(coder: NSCoder)` not. `workflow_ref.sh`: `# per the plan` reported; `echo "per the plan"` not. `api.rs`: `Rule::WorkflowRef.id() == "workflow-ref"`; CLI `--files` case printing exactly the expected `[workflow-ref]` lines.
- Falsifiers: skip doc comments; drop lowercasing; drop leading boundary (COD2 reported); drop trailing boundary (ODE reported); scan strings; delete `from the plan` entry; wire into `--root` only.

### F7 test-writer choices (binding for the coder)
- Quoted `<matched text>` keeps source case (original slice). Quote the leftmost match on the line; `note to the coder` must win over `the coder` when both start at the same place (prefer the longest match at the leftmost position).
- Boundary is checked only at a pattern end that is itself alphanumeric or `_` (so `plan §4` matches `plan §`); `_` is not a boundary.
- `lint_source` must run `workflow-ref` (language fixtures go through it); update its doc.
- Tests: `fixtures/lang/{rust,swift,shell}/workflow_ref.*`, `fixtures/cli/workflow_ref.rs`, `fixtures/repo/src/history.rs`, `fixtures/repo/docs/history.md`; new tests `cli_files_mode_reports_workflow_references_in_their_source_case` (fixtures.rs), `workflow_ref_rule_has_its_documented_id` (api.rs); repo test expects `("src/history.rs", 2, "workflow-ref")`. No new Bazel wiring. `tests/README.md` needs the new entries (technical-writer).

### Follow-up integration inbox
- K1: history wording left in comments: `crates/wolf-core/tests/browser_preview.rs:~529` "no longer non-markdown"; `crates/wolf-core/tests/common/mod.rs:8` "This path is unchanged". Rewrite present-tense in integration.
- K1 → K4: `crates/wolf-core/src/render.rs:1022` doc has `— OD8)` (K4 should catch; verify in residual grep).
- K3: `crates/wolf-ffi/tests/appearance.rs` SKIP section claims an init.lua "line-111 fallback" sets `vim.o.background = "dark"`; current init.lua sets it only in the lighthouse-sibling branch (~154). K1 reworded the same claim in `crates/wolf-core/tests/appearance.rs` to "fallback". Verify against `runtime/nvim/init.lua` and fix both comments to the measured fact.
- K3 FYI: `crates/wolf-ffi/tests/file_actions.rs` "NOT the old empty `[No Name]`" — history wording; rewrite present tense.
- K4: `runtime/nvim/init.lua:228` has `(OQ-1)` — no chunk owns it; fix in integration.
- K4: hyphenated ids (`OQ-1`, `OQ-3`, `OD-a`) are missed by the finder and by F7 `OQ<digits>`. Decision: extend `workflow-ref` with `OD-`/`OQ-` followed by an alphanumeric token (test-writer adds fixture cases), and sweep the tree for `\b(OD|OQ)-[A-Za-z0-9]+\b`.
- K4: milestone-history phrases not caught by the finder: "this unit" (`crates/wolf-gnome/src/actions.rs:137,1390,1512,2329`; `command_action.rs:272,533`; `palette.rs:1774` heading + paragraph; `editor_inset.rs:873`; `geometry.rs:1066` "the floor that shipped before this unit"), "this cut" (`crates/wolf-core/src/markdown.rs:327`, `command.rs:114`), "historical behavior" (`markdown.rs` `WidthMode`). Rewrite present-tense in integration; sweep the tree for `this unit|this cut|shipped before|historical behaviou?r`.
- K5: workflow ids outside the finder remain in `crates/wolf-gnome/tests/smoke_headless.rs`: "pass N" (1436, 3922 embedded-shell #, 19016, 19038, 19267 msg, 19543, 19547, 19573, 19638, 29264, 29267, 29346, 29356, 29361, 29366, 29375, 29397, 29417, 29419, 29427, 29779, 30031 msg, 30325, 30326, 30466, 30486, 31759); M-ids M9/M10/M11/M15/M16/M19/M23/M27 (29264–30791, several in messages); "P2 realized" (29793, 29991, 29996 msg); "Row E" (30338, 30427); verify B1/B2 at 19548–19549 and P1/P2/P3 ~34713 are file-local; "since the WOLF_STATE_DIR isolation landed" (14985 msg). Second sweep needed (one coder, smoke_headless.rs only).
- K6: done (44→1 finder, 50→0 ids). Swift string-edit files for the macOS compile-unverified list: `FrameStatsTests.swift`, `PreviewScrollSeamTests.swift`, `RestoreSeamTests.swift`. History voice left in Swift (`the old`/`no longer`): `PreviewScrollSeam.swift:500`; `PreviewScrollSeamTests.swift:2229`, `:2284` (msg), `:2324–2326` (msg); `AppDelegate.swift:41,203,776`; `StateSeam.swift:51`; `PickerSeam.swift:34`; `PickerSeamTests.swift:42`.
- K6 → K4: `crates/wolf-gnome/src/frame_stats.rs:~1860` desc "the macOS sample ratio" — optional align with the Swift desc.
- K2: done (104→0). `crates/wolf-core/tests/autoread.rs:88`, `:145` cite `swap_exists.rs`, which does not exist (now `swap_recovery.rs`?) — verify and repoint. `crates/wolf-core/tests/tabpages.rs` header bullet says a tabpage switch "does NOT churn GRID ids" while the KEYSTONE doc says "only GRID ids churn" — read the test assertions to establish which is true; fix the wrong comment.
- I1 done (smoke_headless ids removed; self-read tests pass). Left for final pass in `smoke_headless.rs`: "unit" history at ~810, 9259, 17691, 21062 (msg), 21095 (msg), 21118 (msg), 29782 (table), 29803; `SIDEBAR_FILLER_COUNT` doc ~712 "used to hand", ~721 "now hold", ~738 "now write".
- Rule extension done (hyphen ids; 4 falsifiers shown). Remaining `workflow-ref` hit: `crates/wolf-macos/Tests/WolfKeyMapTests/HistorySeamTests.swift:131` "per the architect's call".

## Follow-up review (reviewer: needs revision, no blockers) — routing
- R-A (coder, smoke_headless.rs only): S3 (doc ~6698 "writes exactly one tab per window" → the true reason: one `tabs` key per `WindowState`); S6 (~45 "this unit"/"the X unit" milestone phrases incl. fixture lists ~761–770, ~13194–13227; messages 16374, 16389, 16492, 29165, 29173, 32863, 33294; rethink 27116 and 29165); nit: drop undefined "— F1".."F5" at ~31214–31219.
- R-B (coder, everything else in crates/): S1 ("plan section 3.2" `command_action.rs:289,473,493`; `actions.rs:1005,1966`; `config.rs:288` "(plan: …"); S2 (`crates/wolf-core/tests/appearance.rs:113` "line-111 fallback"); S4 (`vendored_pins.rs` `//!` :12–20, :14, :277, and :307 message wording); S5 (`state_layout.rs:469–470` message); nits `state_seam.rs:55` roadmap ref, `file_search.rs:423`, `mru.rs:340` history voice.
- Rule: add patterns `plan section` and plurals `test-writers`/`test writers`/`the coders` to `workflow-ref` (test-writer first, then coder).

## Follow-up status: COMPLETE on Linux (uncommitted, on top of 0c9ee7f)
Final gate green 174s warm: workspace 1178 passed / 0 failed / 10 ignored; doc-lint 29 passed; `doc-lint --root .` empty (incl. `workflow-ref`). same-code vs PRE: 111 comment-only, 20 with message-string/embedded-shell edits (all planned). init.lua comment-only. Residual sweeps clean. macOS-unverified items added to `notes/macos-followups.md` row 2026-10-24 items (11)–(13).
