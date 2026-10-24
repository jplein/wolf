The documentation in this repo — `specs/`, the README.md, `build.md` and every per-directory README.md — is the source of truth. If the documentation and the implementation differ, that's a bug in one of the two, and must be addressed.

Each source directory in this repo must have a README.md that describes the interfaces and functions of the components of that directory. The mirror directories under `notes/` are not source directories.

When looking for APIs or an understanding of how something works, always start with the documentation, then `notes/`. Treat what `notes/` says as leads to verify against the code.

If you are making more than a trivial change, always use the /orchestrate skill, to make sure that tests are run and the implementation is verified.

Prefer end-to-end and integration tests over mocks. The purpose of this application is to communicate with a Neovim process, so the tests should actually start a Neovim process and communicate with it.

Every new test must actually be run by the command that runs its suite. Bazel does NOT auto-discover test files — each test target enumerates its sources explicitly, so a new test file that is not wired into a target is silently skipped. When you add a test file, also wire it up, then confirm it actually runs.

- **Rust (`crates/*/tests/*.rs`)**: runs under both `bazelisk test` (macOS) and `cargo test` (Linux/Nix). `cargo test` auto-discovers every `tests/*.rs`, but Bazel does not — add a matching `rust_test` target in the crate's `BUILD.bazel`, mirroring a sibling (a real-nvim test needs the staged `//bazel/nvim:nvim_tree` + `//runtime/nvim:init.lua` runfiles, the wired `WOLF_*` env, and `tags = ["no-sandbox"]`; a headless test needs none of those). Because `cargo test` would still pick it up, an unwired file is a macOS-only gap — verify with `bazelisk test`.
- **Swift (`crates/wolf-macos/Tests/WolfKeyMapTests/*.swift`)**: runs only through the `swift_test` target `//crates/wolf-macos:WolfKeyMapTests`, whose `srcs` is an explicit hand-maintained list (NOT a glob) — add every new `*Tests.swift` file to it or the file never compiles into the test binary and never runs. There is no `cargo`-style fallback here, so the `srcs` list is the sole source of truth. (XCTest DOES auto-discover the `test…` methods inside a file that is already listed; the gap is only at the file level. Note the nearby `swift_format_srcs` glob feeds lint/format only, not test execution, so a new file can look "picked up" while still being unrun.)
- **doc-lint (`tools/doc-lint/tests/*.rs`)**: doc-lint is a Cargo workspace of its own, outside the root one, so `cargo test --workspace` never reaches it; the Linux gate runs it with `cargo test --manifest-path tools/doc-lint/Cargo.toml`. Bazel runs it only through the `rust_test` targets in `tools/doc-lint/BUILD.bazel` — add a target for every new file.
- **Lint scripts (`tools/lint/*_test.sh`)**: each needs an `sh_test` in `tools/lint/BUILD.bazel` for macOS and a line in the Linux gate in `build.md`. Neither side finds the script on its own.

## specs/ and notes/

`specs/` is authoritative. A deviation between a spec and the code is a bug in the spec or in the code: decide which, and fix it.

`notes/` is the repo's committed memory, and it is not authoritative. Verify a note against the code before relying on it, and when one is wrong, rewrite it in place. Every `notes/` file carries this line, after its H1 and a blank line, or on line 1 when the file has no H1:

```markdown
Last verified at commit `<40-hex sha>`.
```

The SHA is the commit whose tree you checked the file against — `git rev-parse HEAD` at the moment you verified it — never the commit that adds your edit. Bump it only after re-checking the whole file, not the one paragraph you touched. A file moved into `notes/` keeps the SHA of the last commit that touched it.

Layout:

- `notes/<crate>-decisions.md` — each crate's durable decisions (see the ownership list below).
- `notes/<repo-relative source path>.md` — a mirror of one source file, e.g. `notes/crates/wolf-core/src/nvim.rs.md`, holding the comment content too long for the comment itself. One `##` heading per moved comment, and each heading is linked from its comment.
- Everything else is free-form.

`notes/` is exempt from the markdown checker's changelog rule, because a report records history. Decisions files and mirrors still use the present tense.

## Documentation

Documentation has exactly two jobs. Content that serves neither is the thing to cut.

1. Explain decisions that are not obvious — if a type avoids `NSFoo` because of an Apple bug that thrashes CPU, that belongs in the docs, so a later agent does not "simplify" the code by reaching for `NSFoo`.
2. Describe the purpose of public types and functions, plus anything non-obvious a consumer needs to know.

### Ownership: one file, one owner

- `specs/architecture.md` — the cross-crate view only: component boundaries, the contracts that cross them, and decisions that only make sense at the system level. It names a crate's local constraints and links; it never restates them.
- `specs/<topic>.md` — authoritative, one topic per file, owning the normative content for the files it names; `specs/command-lifecycle.md` and `specs/markdown-stylesheet.md` are examples. A spec may cover a contract internal to one crate, and that crate's README then links it.
- `build.md` — how to build, run and test, per platform: commands, environment, workflows. What a given test target covers belongs in that directory's README, not here.
- `crates/<crate>/README.md` — the outside view only: what the crate is, its public entry points, how it is built and tested, and links to the rest. No per-file entries.
- `notes/<crate>-decisions.md` — the durable "why": cross-cutting constraints, threading model, and decisions with a multi-year half-life. Read this before changing anything in the crate, and verify it like any note.
- `<source-dir>/README.md` — the files in that directory only, one entry each.
- `notes/<path>.md` — the line-level rationale moved out of the comments of the source file at `<path>`. Never that file's purpose or API, which its directory's README owns.

The test for whether something belongs in `specs/architecture.md` or another workspace doc: would it still be true if you deleted one crate? A boundary or a contract survives; a crate's internal constraint does not, and belongs to that crate. The test does not decide whether a contract may be a spec.

Never describe the same file in two documents. Cross-reference by link, never by restating.

A helper subdirectory with no consumer of its own — `tests/common/`, `src/bin/`, `examples/` — is documented by its parent's README rather than getting one of its own. The rule that matters is that exactly one document describes each file, not that every directory has a file.

### Per-file entry template

One `####` heading per file, so entries are greppable and citable. Omit a section that has nothing to say rather than padding it.

```markdown
#### `PickerView.swift`

Purpose: 1–3 sentences.

**Public API** — signatures a consumer calls, plus only what the signature cannot say.

**Decisions** — bullets, each stating the constraint and its reason. Only things a reasonable agent would otherwise undo.

**Gotchas** — non-obvious interactions with other components.
```

### The two-question test

Every sentence must pass one of these. If it passes neither, cut it.

1. Does it explain a decision that a future agent, seeing only the code, would reasonably "simplify" and thereby break? Include the reason, not just the rule.
2. Does a consumer of this type need it, and is it absent from the signature?

Rule of thumb: if 30 seconds of reading the source answers it, do not write it. Write only what reading the source would not tell you.

### Never document

- Private methods, internal call ordering, or control flow the code shows plainly
- Layout constants, colors, timings, magic numbers — cite the named constant instead
- Restatements of a signature in prose
- History: what something used to be, was renamed from, or no longer does. Present tense only; git holds the past. Never write "now", "no longer", "the old", "previously", "this increment". There is no such thing as a limitation "in this increment" — real limitations are present-tense limitations.
- Anything documented in another README. Link to it instead

### Editing an existing doc

Find the entry that already covers the file and edit it in place. Read it first. Never append a new sentence alongside a stale one: if an entry is wrong, rewrite it; if it describes something that was deleted, delete it. Appending is exactly how these docs once grew to the size of the code they describe.

Expect to run net-negative as often as net-positive. A doc that only ever grows is a defect.

Which side to correct depends on what just changed. The rule at the top of this file — documentation is the source of truth — is a design contract: it means a divergence is a bug that must be addressed, not that the prose automatically wins. When the implementation was just changed deliberately, the implementation is the fact and the doc is the thing that drifted. Fix the doc, and say so.

### Format

- Budget: about 200 words per file entry. Exceeding it means you are transcribing implementation — cut, do not restructure.
- Paragraphs are at most 4 sentences, separated by blank lines. No paragraph over about 600 characters.
- No ALL-CAPS emphasis. Use bold sparingly, for genuine warnings only.

`bazel test //tools/lint:markdown` checks the mechanical half of these rules: line length, banned changelog words, ALL-CAPS runs, the per-entry word budget, the `notes/` header line, and a per-file size warning. It is not advisory and not `manual`: it gates on both platforms, under `bazel test //...` on macOS and through `./tools/lint/markdown_all.sh` in the Linux gate. The script also runs standalone — `./tools/lint/markdown_check.sh <files>` — which is how to check a doc on a host without Bazel.

## Code comments

A non-doc comment is at most two lines, and each line is at most 100 characters, measured from the comment marker. Doc comments are exempt: Rust `///`, `//!`, `/** */` and `/*! */`, and Swift `///` and `/** */`. Plain `//`, `#`, `--` and `/* */` are in scope. A line whose text after the marker is a single token with no whitespace, such as a URL or a long path, is exempt from the width limit but still counts as a line.

When a comment is over the limit, apply the two-question test to every sentence, in this order: cut what fails it, shorten what is left to fit in two lines, and only then move the remainder into the file's mirror, `notes/<repo-relative path>.md`, under a `##` heading. The comment keeps the constraint and links the heading on its second line, as `See notes/<path>.md#<anchor>`.

A Markdown path written in a comment is root-relative, and when it starts with `notes/`, `specs/`, `crates/`, `tools/`, `runtime/`, `bazel/` or `resources/`, doc-lint checks that it exists and that its anchor matches a heading. There is no suppression comment. Never convert a comment to a doc comment to escape the limit.

The limits are enforced by [doc-lint](tools/doc-lint/README.md), which gates on both platforms: `//tools/lint:comments` under `bazel test //...` on macOS, and the Linux gate in `build.md`.

## Markdown

When editing Markdown files, avoid adding hard line breaks in a single paragraph.

After editing a Markdown file, reformat it using `prettier --write`.

Note that `.prettierrc` sets `proseWrap: never` for Markdown, so prettier will not break up a long paragraph — it preserves it as a single long line. Shortening paragraphs is an authorial act, not a formatting one.
