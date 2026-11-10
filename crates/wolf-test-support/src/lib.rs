//! The real-`WolfCore` test harness shared by the wolf-core, wolf-ffi and wolf-gnome
//! integration tests.
//!
//! The tests drive a real, hermetic Neovim process (no mocks). This crate resolves the pinned
//! nvim, the captive `init.lua` and Bazel runfiles, gives each core a temp cwd and the process a
//! throwaway `WOLF_STATE_DIR`, builds `CoreConfig`s, waits on the wake channel, and opens and
//! reads terminal sessions.
//!
//! Each consumer re-exports what its tests use through its own `tests/common/mod.rs`, and keeps
//! its crate-specific helpers there. It is a dev-dependency only, never a normal dependency of a
//! shipped crate.

mod core_config;
mod resolve;
mod terminal;
mod wake;

use std::time::Duration;

pub use core_config::{config, config_in, fresh_cwd, hermetic_state_dir, unique_temp_dir};
pub use resolve::{
    init_lua_path, nvim_bin_or_skip, require_nvim_bin, resolve_runfile_value, resolve_test_runfile,
    rg_bin_or_skip,
};
pub use terminal::{
    frame_of, frame_text, open_session, pin_default_shell, poll_frame, sh_script, wait_for_text,
};
pub use wake::{recording_wake_channel, start_core_sized, wake_channel, FlushWaiter, WakeLog};

/// Default grid width used by most tests, in columns.
pub const DEFAULT_COLS: u16 = 80;
/// Default grid height used by most tests, in rows.
pub const DEFAULT_ROWS: u16 = 24;

/// Generous timeout to absorb nvim `--embed` startup + first redraw flush.
/// nvim startup is not instantaneous and redraw is never synchronous, so we wait
/// well beyond the expected latency before declaring failure.
pub const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Gap between two tries of a polling helper, such as [`poll_frame`].
pub const POLL_INTERVAL: Duration = Duration::from_millis(25);
