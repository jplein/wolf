# doc-lint/src

The library and binary behind [doc-lint](../README.md), which owns the rule definitions, the language table and the CLI contract. The `lexer/` subdirectory has no consumer outside this crate, so its files are described here.

## Files

#### `lib.rs`

Purpose: the public API, the language table in code, and the two drivers that combine the rule modules.

**Public API**

- `lint_source(display_path, text, lang) -> Vec<Finding>`: the comment limits and `workflow-ref`, with no file system access.
- `check_files(cwd, files, cfg) -> io::Result<Vec<Finding>>`: the comment limits, `workflow-ref`, `doc-ref` and `doc-anchor`, references resolved against `cwd`. An unreadable file is an error rather than a finding.
- `check_root(root, cfg) -> Vec<Finding>`: every rule over a walk of `root`; unreadable files are skipped.
- `language_for(path)`, `WalkConfig::repo_default()`, `Finding`, `Rule::id()`, `MAX_COMMENT_LINES` and `MAX_COMMENT_WIDTH`, plus the re-exported `run`, `strip_comments`, `heading_slug` and `missing_coverage`.

**Decisions**

- `language_for` decides from the file name alone, never the contents, so the Bazel glob patterns can mirror it exactly.

#### `main.rs`

Purpose: the `doc-lint` binary, a call to `run` with the process's arguments and streams.

#### `cli.rs`

Purpose: `run(args, out, err) -> i32`, which parses the four modes, prints findings and summaries, and maps outcomes to exit codes.

**Gotchas**

- `--files` checks that every path is a regular file before linting any, so one typo exits 2 instead of linting the rest.

#### `rules.rs`

Purpose: groups comments into blocks and applies `comment-lines` and `comment-width`.

#### `refs.rs`

Purpose: finds the root-relative Markdown paths written in comments and applies `doc-ref` and `doc-anchor`.

**Gotchas**

- The reference scan reads the raw comment text, markers included, so a path right after `//` with no space is not at a boundary and is not a reference.

#### `workflow.rs`

Purpose: `workflow-ref`, over the raw text of every comment line.

**Decisions**

- The patterns live only in the `PATTERNS` table of string literals. `--root .` scans doc-lint's own comments, so a comment that spells a pattern, even to explain it, is a finding.
- The match runs on an ASCII-lowercased copy, which keeps every byte offset, so the quoted text is sliced from the original line.
- A line is deduplicated against the last finding only, which holds because `comments` arrive in source order. Two comments on one line therefore give one finding, as the sidecar comparison requires.

#### `markdown.rs`

Purpose: heading anchors, link extraction and the `md-link` and `md-anchor` rules.

**Public API** — `heading_slug(text) -> String`, one heading's anchor before the duplicate suffix.

**Gotchas**

- A bare `<…>` autolink is never a relative link, since CommonMark requires a scheme there. Treating it as one would flag every `<crate>`-style placeholder in prose.

#### `notes.rs`

Purpose: `notes-orphan` and `notes-unlinked` for `notes/<P>.md` mirrors. It lexes `<P>` itself rather than reusing the walk's result, so a source inside an excluded prefix is still read.

#### `coverage.rs`

Purpose: `bazel-coverage`, reported against each package's `BUILD.bazel`.

**Public API** — `missing_coverage(package_dirs, lint_build_text) -> Vec<String>`: the full labels absent from the text, `""` being the root package.

#### `walk.rs`

Purpose: the sorted, root-relative file list for `--root`. Symlinks, to files or directories, are never followed, so a `result` link into the Nix store is never walked.

#### `strip.rs`

Purpose: `strip_comments` and the first-difference report behind `same-code`.

**Decisions**

- Blank lines are dropped only outside strings, and trailing whitespace is trimmed only there, so an edit inside a multi-line string is still a difference.

### `lexer/`

#### `lexer/mod.rs`

Purpose: dispatches to the language lexers and turns their raw comment spans into `Comment`s with per-line width text, body text and whether each line counts.

**Decisions**

- The lexers work on bytes. Every delimiter is ASCII, and a non-ASCII byte counts as an identifier byte, so a Unicode identifier stays one word and a slice never splits a char.
- CRLF is joined to LF before lexing, so a `\r` never counts toward width.
- Each lexer also records string spans, which `strip` needs to keep strings verbatim.

#### `lexer/rust.rs`

Purpose: Rust comments, strings with `b`, `c` and raw prefixes, and char literals.

**Gotchas**

- After a `'`, a backslash or a closing `'` one char later makes a char literal; anything else is a lifetime or label. `r#ident` is an identifier, not a raw string.

#### `lexer/swift.rs`

Purpose: Swift comments and strings, including `\( )` interpolation holding further strings, `"""` and raw `#"…"#` strings with `\#(`. A `#` not followed by `"` is a directive such as `#if`.

#### `lexer/c_like.rs`

Purpose: C, and CSS, which has no `//` comment. Neither nests block comments.

#### `lexer/lua.rs`

Purpose: Lua comments, quoted strings and long brackets of any level.

#### `lexer/shell.rs`

Purpose: shell comments, quoting, parameter expansion and heredocs.

**Gotchas**

- `<<` inside `(( ))` or `$(( ))` is a shift, never a heredoc, so `$(( x << name ))` cannot swallow the rest of the file. Outside arithmetic, a heredoc word must be quoted or start with a letter or `_`.
- Comments inside `"$( … )"` are not recognised; the substitution is skipped as part of the string.

#### `lexer/starlark.rs`

Purpose: Starlark comments and the four quote forms; docstrings are strings.

#### `lexer/nix.rs`

Purpose: Nix comments, `"…"` and `''…''` strings with their escapes, and `${ }` antiquotes.

**Gotchas**

- An identifier may contain `'` and `-`, so `x''` is consumed as one identifier and never opens an indented string.

#### `lexer/toml.rs`

Purpose: TOML comments and its four string forms; a multi-line closer may absorb up to two extra quotes.

#### `lexer/hashline.rs`

Purpose: ignore and rc files, where only a line starting with `#` after optional blanks is a comment.
