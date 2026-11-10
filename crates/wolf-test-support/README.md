# wolf-test-support

The real-`WolfCore` test harness shared by the integration tests of [`wolf-core`](../wolf-core/tests/README.md), [`wolf-ffi`](../wolf-ffi/tests/README.md) and [`wolf-gnome`](../wolf-gnome/tests/README.md). It resolves the pinned Neovim, the captive `init.lua` and Bazel runfiles, gives each core a temp cwd and the test binary a throwaway `WOLF_STATE_DIR`, builds `CoreConfig`s, waits on the wake channel, and opens and reads terminal sessions.

Each consumer's `tests/common/mod.rs` re-exports what its tests use and keeps that crate's own helpers, which its tests README describes.

## How a consumer reaches it

- Cargo: a `[dev-dependencies]` path entry in the consumer's `Cargo.toml`. wolf-gnome has no Bazel target, so this is its only route.
- Bazel: `//crates/wolf-test-support:wolf_test_support`, a `testonly` `rust_library`. Every `rust_test` whose `srcs` lists `tests/common/mod.rs` names it in `deps` by hand, because `all_crate_deps` never lists a workspace member.

It is never a normal dependency of a shipped crate. Bazel enforces that through `testonly`; Cargo enforces nothing, so it is a rule to keep.

The `wolf-core-deps` gate lists its dependency graph beside wolf-core's, for the reason [the one hard rule](../../notes/wolf-core-decisions.md#the-one-hard-rule) gives. It depends on wolf-core and `std` alone.

**Gotchas**

- wolf-core's own `#[cfg(test)]` modules cannot use it. It links wolf-core's library, whose types are distinct from those of the crate under unit test.

## Public API surface

Everything is at the crate root as `wolf_test_support::…`. [`src/README.md`](src/README.md) has the detail.

| Module           | Entry points                                                                                                               |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `lib.rs`         | `DEFAULT_COLS`, `DEFAULT_ROWS`, `FLUSH_TIMEOUT`, `POLL_INTERVAL`                                                           |
| `resolve.rs`     | `require_nvim_bin`, `nvim_bin_or_skip`, `rg_bin_or_skip`, `init_lua_path`, `resolve_runfile_value`, `resolve_test_runfile` |
| `core_config.rs` | `config`, `config_in`, `fresh_cwd`, `unique_temp_dir`, `hermetic_state_dir`                                                |
| `wake.rs`        | `FlushWaiter`, `wake_channel`, `recording_wake_channel`, `WakeLog`, `start_core_sized`                                     |
| `terminal.rs`    | `pin_default_shell`, `sh_script`, `open_session`, `frame_of`, `frame_text`, `poll_frame`, `wait_for_text`                  |

## Building and testing

It has no tests of its own: the consumers' suites exercise it.

```sh
nix develop --command cargo clippy -p wolf-test-support -- -D warnings   # Linux
bazel build //crates/wolf-test-support:wolf_test_support                 # macOS
```

On macOS, clippy and rustfmt reach the crate through `//tools/lint:clippy_test_support` and `:rustfmt_test_support`; see [`tools/lint`](../../tools/lint/README.md).
