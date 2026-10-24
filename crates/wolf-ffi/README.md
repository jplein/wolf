# wolf-ffi

The swift-bridge surface over [`wolf-core`](../wolf-core/README.md). It exposes core's API to Swift as a C-and-Swift ABI, and it is the only crate swift-bridge touches.

**Read [wolf-ffi-decisions.md](../../notes/wolf-ffi-decisions.md) before changing anything in this crate.** Most of what is here is an ABI contract, so a change that compiles can still break the Swift side silently.

## Layout

| Path                        | Contents                                                         |
| --------------------------- | ---------------------------------------------------------------- |
| [`src/`](src/README.md)     | `lib.rs`, the whole bridge surface, plus the codegen binary.     |
| `generated/`                | Committed swift-bridge output plus the hand-written wake header. |
| [`tests/`](tests/README.md) | Integration tests, most driving a real pinned Neovim.            |

## Bazel targets

| Target                | What it is                                                                                                                                               |
| --------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `:wolf_ffi`           | The `rlib`, used by the Rust integration tests.                                                                                                          |
| `:wolf_ffi_staticlib` | `libwolf_ffi.a`, linked into the app.                                                                                                                    |
| `:wolf_ffi_cc`        | A `cc_library` exposing the generated headers and the wake header as the `WolfFFI` Clang module, and linking the archive. Consumed by the Swift targets. |
| `:generated_swift`    | A filegroup of the generated Swift glue, compiled into wolf-macos's `wolf_key_map` module.                                                               |
| `:gen_bindings`       | The codegen binary. Run with `bazel run`, never built as a genrule — see [wolf-ffi-decisions.md](../../notes/wolf-ffi-decisions.md).                     |

Every other target in `BUILD.bazel` is an integration test named after its source file; [`tests/README.md`](tests/README.md) covers what each one drives and what it needs staged.

## Regenerating

```sh
bazel run //crates/wolf-ffi:gen_bindings                       # rewrites generated/, then commit
CARGO_BAZEL_REPIN=1 bazel build //crates/wolf-ffi:wolf_ffi     # after a Cargo dependency change
```

## Testing

```sh
bazel test //crates/wolf-ffi:bridge_smoke   # the editor round-trip against a real pinned nvim
cargo test -p wolf-ffi                      # the same tests on the Linux and Nix path
```

Tests drive a real Neovim rather than a mock. Bazel does not discover test files: a new `tests/*.rs` needs a matching `rust_test` target with the right runfiles staged, or it runs only under `cargo test` and is a silent macOS-only gap. See [wolf-ffi-decisions.md](../../notes/wolf-ffi-decisions.md) for the staging and skip rules.
