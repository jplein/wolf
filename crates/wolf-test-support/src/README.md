# wolf-test-support/src

The harness's modules. The crate-level view, who consumes it and how, is in [`../README.md`](../README.md).

## Files

#### `lib.rs`

Purpose: the crate root. It declares the grid size most tests attach with, `DEFAULT_COLS` by `DEFAULT_ROWS`, and the `FLUSH_TIMEOUT` and `POLL_INTERVAL` the waits and polls here use.

**Decisions**

- The modules are private and every public item is re-exported flat at the root, so a consumer's `pub use wolf_test_support::{…}` names no module.

#### `resolve.rs`

Purpose: resolves the pinned `nvim` and `rg`, the captive `init.lua`, and Bazel runfiles to absolute paths.

**Public API**

- `require_nvim_bin()` panics when no Neovim resolves, for wolf-core's and wolf-gnome's test bodies, which handle no skip. `nvim_bin_or_skip()` prints a skip banner and returns `None`, and its caller must `return`, never panic or assert success. `rg_bin_or_skip()` follows the same contract. A skip passes, so green does not by itself prove the binary ran.
- `nvim_bin_or_skip()` also sets up the throwaway `WOLF_STATE_DIR` ([`core_config.rs`](#core_configrs)), so a test that starts a core with no `CoreConfig` from this crate, as through `wolf_ffi_start`, calls it first.
- `resolve_runfile_value(value)` takes an absolute or runfiles-relative path. `resolve_test_runfile(env_key)` reads it from a variable, unset and empty alike giving `None`.

**Decisions**

- An explicit `WOLF_NVIM_BIN` is trusted only when absolute and existing; otherwise the staged `WOLF_TEST_NVIM_DIR` tree wins. Bazel injects a relative value, and nvim is spawned with its cwd a temp dir, where a relative path fails.
- Both nvim helpers publish the result in `WOLF_NVIM_BIN`, once per binary behind a `OnceLock` and only when it differs. `WolfCore::start` takes its binary from that variable alone, and a per-test `setenv` would race the parallel tests' `getenv`.
- `rg_bin_or_skip()` publishes an absolute `WOLF_RG_BIN`, since Bazel injects a runfiles-relative one.
- `init_lua_path()` prefers the staged `WOLF_TEST_INIT_LUA`. Under Bazel `CARGO_MANIFEST_DIR` is a sandbox path absent at test time, and nvim given a missing `-u` file hangs on a `Press ENTER` prompt.

#### `core_config.rs`

Purpose: temp directories, the hermetic `WOLF_STATE_DIR`, and the `CoreConfig` builders.

**Public API**

- `config(cols, rows)` and `config_in(cwd, cols, rows)` are the state-dir funnel: build every `CoreConfig` through them, since a hand-built literal skips it. `config` gives each core its own `fresh_cwd()`.
- `hermetic_state_dir()` returns `wolf_core::state_dir()` after the same setup, for a test that seeds or reads the MRU store the core uses.
- `unique_temp_dir(prefix)` and `fresh_cwd()` make a new directory and never delete it.

**Decisions**

- The funnel points `WOLF_STATE_DIR` at a fresh `wolf-test-state-*` dir, once per binary and only if unset. Without it `state_dir()` is the user's real Wolf state dir under Bazel and Cargo alike, and every test would source their `init_user.lua` and write swap and MRU files there.
- The write runs inside a `OnceLock`'s `get_or_init`, because `setenv` races `getenv` on libtest's parallel threads. A test that sets its own `WOLF_STATE_DIR` first keeps it; one that removes it must be alone in its binary, since nothing sets it again for its neighbours.
- Nothing is deleted: a cwd is nvim's working dir and search root, and neither it nor the state dir may vanish under a running core.
- Creation is an exclusive `create_dir`, so a name collision panics rather than letting two tests share a directory.

#### `wake.rs`

Purpose: the wake channel a test core signals, the waits built on it, and `start_core_sized`, which starts a bare core in a fresh cwd and returns after its attach flush.

**Public API**

- `wake_channel()` pairs a `WakeFn` with the `FlushWaiter` it signals. `recording_wake_channel()` also returns a `WakeLog` of each wake's instant.
- `wait_for_flush` and `wait_for_initial_flush` panic after their timeout; `wait_for_wake` returns `false`. `drain` discards queued wakes.
- `wait_for_open(core, path, baseline, expect_text, timeout)` waits for an opened file to be current and drawn. Read `baseline` after a blocking RPC that follows startup.

**Decisions**

- The flush waits key on the snapshot's `seq`, not on the wake count. An out-of-band wake (`wolf_current_file`, a terminal frame, a liveness change) fires the same callback without bumping `seq`, so it must not satisfy a wait for a redraw; `wait_for_wake` observes it.
- Use `wait_for_initial_flush` right after `WolfCore::start`. The attach flush lands during `start`, so `wait_for_flush` baselines after it and blocks until the timeout.

**Gotchas**

- Neither `seq` nor `current_file` alone proves an open drew: treesitter's asynchronous parse can push `seq` past the baseline before the `:edit`, and `current_file` is notified before the open's frame. The startup buffer's row 0 is blank, so `expect_text` requires row 0 to show text.

#### `terminal.rs`

Purpose: the terminal-session helpers: open a session, read its frame, and poll until it shows something. `poll_frame` and `wait_for_text` fail with the frame's text rather than hang.

**Decisions**

- `sh_script` runs `/bin/sh`, never `$SHELL`, because the tests type for POSIX `sh` output.
- `pin_default_shell()` sets `SHELL=/bin/sh` once per process, for a test that opens a default-shell session: the drawer, a typed `:terminal` or a restored terminal leaf. Call it before starting a core, so no session reads `SHELL` first.
