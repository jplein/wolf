# tools

Build-time developer tooling (not part of any shipped artifact).

- [`lint/`](lint/README.md) — advisory (non-gating) Bazel lint/format targets for the macOS host: clippy + rustfmt (Rust) and swift-format (Swift).
- [`wolf-pointer-inject/`](wolf-pointer-inject/README.md) — a long-lived Wayland `zwlr_virtual_pointer_v1` client that holds the seat's pointer capability open and injects motion / button / axis events; Linux-only, built by `flake.nix` into the devShell, used by the `wolf-gnome` cage pointer smokes.
- `wolf-package-smoke.sh` — the packaged-artifact evidence run: it builds `.#wolf-gnome`, prints the store path it resolved, and drives every `#[ignore]`d test that reads or launches that path. An ignored test is honest only when exactly one command runs it and always establishes its precondition; this is that command. Linux-only, run from anywhere, and not to be run beside another `cargo test` — two `cage` compositors contend for one Wayland socket. See [`build.md`](../build.md).
