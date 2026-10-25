# wolf-ffi decisions

Last verified at commit `f7a53aff023b83e348f874e99fa44efeb33502db`.

The durable constraints behind the Rust-to-Swift bridge. Read this before changing anything here — most of the surface is an ABI contract, so a change that compiles can still break a consumer silently. Per-file detail lives in [`src/README.md`](../crates/wolf-ffi/src/README.md).

## Containment

swift-bridge is confined to this crate's dependency graph and never enters [`wolf-core`](../crates/wolf-core/README.md)'s tree. That is what keeps core's one hard rule enforceable; see [wolf-core-decisions.md](wolf-core-decisions.md).

## The wake is a registered C function pointer, not `extern "Swift"`

```c
void wolf_ffi_set_wake(void (*wake)(uint64_t token));   // generated/wolf_ffi_wake.h
```

The obvious design is an `extern "Swift"` symbol, and it is wrong here: a Rust-only integration test could never resolve one at link time. Registration keeps `wolf_ffi_start` linkable and runnable with no Swift runtime present, which is what lets the real-nvim tests drive the bridge at all. An unregistered wake is a harmless no-op.

Each callback carries the firing core's per-project wake token, a `u64` minted in `wolf_ffi_start`, so one process-global callback routes each wake to the right project without clobbering. The token identifies a project — one captive Neovim shared by N views — and the front-end fans it out to that project's current view.

The wake fires on a tokio worker thread and must not touch the `!Send` handle, which is precisely why identity crosses the boundary as a plain `Send` scalar rather than a pointer. The registered thunk must stay `@convention(c)`, so it can capture nothing but globals.

## Wake channels

There are four, each independently registered:

| Registration                   | Fires for                |
| ------------------------------ | ------------------------ |
| `wolf_ffi_set_wake`            | The editor render stream |
| `wolf_ffi_set_filesearch_wake` | File-picker traversal    |
| `wolf_ffi_set_tree_wake`       | File-tree traversal      |
| `wolf_ffi_set_search_wake`     | Content-search results   |

This distinction governs regeneration. An additive `WolfCoreHandle` method rides an existing wake and its regen touches only `wolf-ffi.swift` and `wolf-ffi.h`, leaving `wolf_ffi_wake.h` byte-identical. Only a genuinely new streaming channel adds a registration, and that is the change worth scrutiny — it means a new callback the front-end must register and a new thread-safety question to answer.

## Discriminant tables are append-only

The `u8` discriminant modules in `src/lib.rs` (`command_kind`, `palette_row_kind`, `liveness`, the mouse, scroll, split, and focus enums) are the ABI. Swift decodes raw integers, so renumbering an existing value silently changes behavior on the other side of the boundary rather than failing to compile.

Append at the end, never renumber or reuse. `command_kind` is contiguous from `0` and its upper bound is pinned by the `//crates/wolf-ffi:commands` test, which is what turns a renumbering into a test failure.

## Generated bindings are committed

`generated/` holds the swift-bridge output plus two hand-written files:

- `SwiftBridgeCore.swift` / `SwiftBridgeCore.h` — the shared swift-bridge runtime.
- `wolf-ffi.swift` / `wolf-ffi.h` — this crate's bridge.
- `wolf_ffi_wake.h` — the hand-written wake registrations.
- `module.modulemap` — exposes the C headers to Swift as the `WolfFFI` module, matching the `import WolfFFI` line `gen_bindings` prepends to the generated Swift and the `swift_interop_hint` in `BUILD.bazel`.

```sh
bazel run //crates/wolf-ffi:gen_bindings                       # rewrites generated/, then commit
CARGO_BAZEL_REPIN=1 bazel build //crates/wolf-ffi:wolf_ffi     # after a Cargo dependency change
```

`gen_bindings` must be a `bazel run` binary rather than a genrule: it writes in-tree by reading `BUILD_WORKSPACE_DIRECTORY`, which only `bazel run` provides, and a genrule cannot write into the source tree.

## Testing posture

Tests drive a real pinned Neovim, never a mock, because the thing under test is a live RPC round-trip. They carry `tags = ["no-sandbox"]` since spawning a child nvim does not work under the macOS build sandbox; results are still cached.

A test loudly skips only when neither an explicit `WOLF_NVIM_BIN` nor the staged nvim tree is available. Staging is per-target `data` plus `$(rootpath)` env vars, so a new real-nvim test needs the runfiles wired or it skips silently on the Bazel host. See [`tests/README.md`](../crates/wolf-ffi/tests/README.md).

The harness keeps tests off the user's Wolf state dir by publishing a throwaway `WOLF_STATE_DIR` from `nvim_bin_or_skip()`, not by changing `HOME` or adding a Bazel `env`. A Bazel `env` value cannot expand to a per-run path, so it would share one fixed directory across concurrently running targets, and it does nothing for `cargo test`, which inherits the user's `HOME`. A scratch `HOME` for every target would also break wolf-core's `swap_recovery`, which needs the real one through `env_inherit`. `//crates/wolf-ffi:state_isolation` guards the funnel.
