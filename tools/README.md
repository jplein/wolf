# tools

Build-time developer tooling (not part of any shipped artifact).

- [`doc-lint/`](doc-lint/README.md) — the comment-limit, doc-reference, `notes/` mirror and Markdown link checker; a standalone Rust workspace run by both platforms' gates.
- [`lint/`](lint/README.md) — the Bazel lint and format targets for the macOS host: clippy and rustfmt (Rust), swift-format (Swift), the markdown checker and the doc-lint comment check.
- [`wolf-pointer-inject/`](wolf-pointer-inject/README.md) — a long-lived Wayland `zwlr_virtual_pointer_v1` client that holds the seat's pointer capability open and injects motion / button / axis events; Linux-only, built by `flake.nix` into the devShell, used by the `wolf-gnome` cage pointer smokes.
- `wolf-package-smoke.sh` — the packaged-artifact evidence run: it builds `.#wolf-gnome`, prints the store path it resolved, and drives every `#[ignore]`d test that reads or launches that path. An ignored test is honest only when exactly one command runs it and always establishes its precondition; this is that command. Linux-only, run from anywhere, and never beside another `cargo test`, for the reason [`build.md`](../build.md#building-and-testing) gives.
