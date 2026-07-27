The documentation in this repo, starting with the README.md, is the source of truth. If the documentation and the implementation differ, that's a bug, and must be addressed.

Each source directory in this repo must have a README.md that describes the interfaces and functions of the components of that directory and its subdirectories.

When looking for APIs or an understanding of how something works, always start with the documentation.

If you are making more than a trivial change, always use the /orchestrate skill, to make sure that tests are run and the implementation is verified.

Prefer end-to-end and integration tests over mocks. The purpose of this application is to communicate with a Neovim process, so the tests should actually start a Neovim process and communicate with it.

Every new test must actually be run by the command that runs its suite. Bazel does NOT auto-discover test files — each test target enumerates its sources explicitly, so a new test file that is not wired into a target is silently skipped. When you add a test file, also wire it up, then confirm it actually runs.

- **Rust (`crates/*/tests/*.rs`)**: runs under both `bazelisk test` (macOS) and `cargo test` (Linux/Nix). `cargo test` auto-discovers every `tests/*.rs`, but Bazel does not — add a matching `rust_test` target in the crate's `BUILD.bazel`, mirroring a sibling (a real-nvim test needs the staged `//bazel/nvim:nvim_tree` + `//runtime/nvim:init.lua` runfiles, the wired `WOLF_*` env, and `tags = ["no-sandbox"]`; a headless test needs none of those). Because `cargo test` would still pick it up, an unwired file is a macOS-only gap — verify with `bazelisk test`.
- **Swift (`crates/wolf-macos/Tests/WolfKeyMapTests/*.swift`)**: runs only through the `swift_test` target `//crates/wolf-macos:WolfKeyMapTests`, whose `srcs` is an explicit hand-maintained list (NOT a glob) — add every new `*Tests.swift` file to it or the file never compiles into the test binary and never runs. There is no `cargo`-style fallback here, so the `srcs` list is the sole source of truth. (XCTest DOES auto-discover the `test…` methods inside a file that is already listed; the gap is only at the file level. Note the nearby `swift_format_srcs` glob feeds lint/format only, not test execution, so a new file can look "picked up" while still being unrun.)

## Markdown

When editing Markdown files, avoid adding hard line breaks in a single paragraph.

After editing a Markdown file, reformat it using `prettier --write`.
