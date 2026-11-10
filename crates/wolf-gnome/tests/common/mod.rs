//! Shared helpers for `wolf-gnome` integration tests, re-exported from the `wolf_test_support`
//! crate that the wolf-core and wolf-ffi tests share.
//!
//! The real-nvim tests drive a real, hermetic Neovim process (no mocks). [`require_nvim_bin`]
//! panics rather than skips when no pinned Neovim resolves, so a misconfigured environment
//! surfaces at once. [`config_in`] is the state-dir funnel: every core built from its config
//! runs against a throwaway `WOLF_STATE_DIR`, never the user's own Wolf state dir.

#![allow(unused_imports)] // Not every helper is used by every test binary.

pub use wolf_test_support::{
    config, config_in, frame_of, frame_text, open_session, poll_frame, require_nvim_bin, sh_script,
    start_core_sized, unique_temp_dir, wait_for_text, wake_channel, FlushWaiter, DEFAULT_COLS,
    DEFAULT_ROWS, FLUSH_TIMEOUT,
};
