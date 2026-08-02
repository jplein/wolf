The documentation in this repo, starting with the README.md, is the source of truth. If the documentation and the implementation differ, that's a bug, and must be addressed.

Each source directory in this repo must have a README.md that describes the interfaces and functions of the components of that directory.

When looking for APIs or an understanding of how something works, always start with the documentation.

If you are making more than a trivial change, always use the /orchestrate skill, to make sure that tests are run and the implementation is verified.

Prefer end-to-end and integration tests over mocks. The purpose of this application is to communicate with a Neovim process, so the tests should actually start a Neovim process and communicate with it.

Every new test must actually be run by the command that runs its suite. Bazel does NOT auto-discover test files — each test target enumerates its sources explicitly, so a new test file that is not wired into a target is silently skipped. When you add a test file, also wire it up, then confirm it actually runs.

- **Rust (`crates/*/tests/*.rs`)**: runs under both `bazelisk test` (macOS) and `cargo test` (Linux/Nix). `cargo test` auto-discovers every `tests/*.rs`, but Bazel does not — add a matching `rust_test` target in the crate's `BUILD.bazel`, mirroring a sibling (a real-nvim test needs the staged `//bazel/nvim:nvim_tree` + `//runtime/nvim:init.lua` runfiles, the wired `WOLF_*` env, and `tags = ["no-sandbox"]`; a headless test needs none of those). Because `cargo test` would still pick it up, an unwired file is a macOS-only gap — verify with `bazelisk test`.
- **Swift (`crates/wolf-macos/Tests/WolfKeyMapTests/*.swift`)**: runs only through the `swift_test` target `//crates/wolf-macos:WolfKeyMapTests`, whose `srcs` is an explicit hand-maintained list (NOT a glob) — add every new `*Tests.swift` file to it or the file never compiles into the test binary and never runs. There is no `cargo`-style fallback here, so the `srcs` list is the sole source of truth. (XCTest DOES auto-discover the `test…` methods inside a file that is already listed; the gap is only at the file level. Note the nearby `swift_format_srcs` glob feeds lint/format only, not test execution, so a new file can look "picked up" while still being unrun.)

## Documentation

Documentation has exactly two jobs. Content that serves neither is the thing to cut.

1. Explain decisions that are not obvious — if a type avoids `NSFoo` because of an Apple bug that thrashes CPU, that belongs in the docs, so a later agent does not "simplify" the code by reaching for `NSFoo`.
2. Describe the purpose of public types and functions, plus anything non-obvious a consumer needs to know.

### Ownership: one file, one owner

- `crates/<crate>/README.md` — the outside view only: what the crate is, its public entry points, how it is built and tested, and links to the rest. No per-file entries.
- `crates/<crate>/DECISIONS.md` — the durable "why": cross-cutting constraints, threading model, and decisions with a multi-year half-life. Read this before changing anything in the crate.
- `<source-dir>/README.md` — the files in that directory only, one entry each.

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

### Format

- Budget: about 200 words per file entry. Exceeding it means you are transcribing implementation — cut, do not restructure.
- Paragraphs are at most 4 sentences, separated by blank lines. No paragraph over about 600 characters.
- No ALL-CAPS emphasis. Use bold sparingly, for genuine warnings only.

`bazel test //tools/lint:markdown` checks the mechanical half of these rules: line length, banned changelog words, ALL-CAPS runs, the per-entry word budget, and a per-file size warning. It is advisory and `manual`-tagged, like the rest of `//tools/lint`. The script also runs standalone — `./tools/lint/markdown_check.sh <files>` — which is how to check a doc on a host without Bazel.

## Markdown

When editing Markdown files, avoid adding hard line breaks in a single paragraph.

After editing a Markdown file, reformat it using `prettier --write`.

Note that `.prettierrc` sets `proseWrap: never` for Markdown, so prettier will not break up a long paragraph — it preserves it as a single long line. Shortening paragraphs is an authorial act, not a formatting one.
