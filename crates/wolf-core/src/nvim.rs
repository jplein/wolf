//! Spawning and supervising the captive, hermetic Neovim child process.
//!
//! Neovim is located *only* via `WOLF_NVIM_BIN` (exported by the flake devShell),
//! never PATH and never by scanning `/nix/store`. It is launched
//! `--embed --clean -u <init.lua> -i NONE -n` with `NVIM_APPNAME` and the
//! `XDG_*` dirs pinned under a private temp dir so it cannot read the user's
//! config/state. All RPC is generic/untyped to insulate us from nvim-rs's older
//! typed API.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use nvim_rs::compat::tokio::Compat;
use nvim_rs::error::LoopError;
use nvim_rs::{create::tokio::new_child_cmd, Neovim};
use rmpv::Value;
use tokio::process::{ChildStdin, Command};
use tokio::task::JoinHandle;

use crate::command::COMMAND_SPECS;
use crate::redraw::RedrawHandler;

/// Application name pinned for the captive nvim, isolating its state dirs.
const NVIM_APPNAME: &str = "wolf";

/// How long to wait for the child to exit after `qa!` before force-killing.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// How long to wait for the `qa!` RPC itself before moving on (nvim exits rather
/// than replying, so this normally elapses or errors — both are fine).
const QUIT_GRACE: Duration = Duration::from_secs(2);

/// Errors from spawning or driving the captive Neovim.
#[derive(Debug, thiserror::Error)]
pub enum NvimError {
    /// `WOLF_NVIM_BIN` was unset or empty.
    #[error("WOLF_NVIM_BIN is not set; the pinned nvim binary must be provided by the flake devShell")]
    BinNotSet,
    /// Failed to create the isolated state directories.
    #[error("failed to set up isolated nvim state dir: {0}")]
    StateDir(#[source] std::io::Error),
    /// Failed to spawn the child process / establish the RPC channel.
    #[error("failed to spawn nvim child process: {0}")]
    Spawn(#[source] std::io::Error),
    /// A generic RPC call failed.
    #[error("nvim RPC call failed: {0}")]
    Rpc(String),
    /// Attaching as a UI failed.
    #[error("nvim_ui_attach failed: {0}")]
    Attach(String),
}

/// Configuration for launching the captive Neovim.
#[derive(Clone, Debug)]
pub struct NvimConfig {
    /// Absolute path to the pinned nvim binary (from `WOLF_NVIM_BIN`).
    pub nvim_bin: PathBuf,
    /// Path to the in-repo `runtime/nvim/init.lua`.
    pub init_lua: PathBuf,
    /// `NVIM_APPNAME` value.
    pub appname: String,
    /// Root for the isolated `XDG_*` dirs (a private temp dir).
    pub state_root: PathBuf,
    /// Working directory the captive nvim is launched in (so its `getcwd()`
    /// starts here rather than inheriting the host process's cwd).
    pub cwd: PathBuf,
    /// Initial UI column count.
    pub initial_cols: u16,
    /// Initial UI row count.
    pub initial_rows: u16,
    // Kept alive so the temp dir is not deleted while nvim runs.
    _state_guard: std::sync::Arc<tempfile::TempDir>,
}

impl NvimConfig {
    /// Builds a config from the environment: reads `WOLF_NVIM_BIN` and creates a
    /// private temp dir to host the isolated `XDG_*` state. `cwd` is the working
    /// directory the captive nvim will be launched in (see [`spawn`]).
    pub fn from_env(
        init_lua: PathBuf,
        cwd: PathBuf,
        cols: u16,
        rows: u16,
    ) -> Result<Self, NvimError> {
        let bin = std::env::var("WOLF_NVIM_BIN")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .ok_or(NvimError::BinNotSet)?;

        let temp = tempfile::Builder::new()
            .prefix("wolf-nvim-state-")
            .tempdir()
            .map_err(NvimError::StateDir)?;
        let state_root = temp.path().to_path_buf();

        Ok(Self {
            nvim_bin: PathBuf::from(bin),
            init_lua,
            appname: NVIM_APPNAME.to_string(),
            state_root,
            cwd,
            initial_cols: cols,
            initial_rows: rows,
            _state_guard: std::sync::Arc::new(temp),
        })
    }
}

/// A live captive Neovim: the RPC handle plus the io-loop join handle and child.
pub struct NvimProcess {
    nvim: Neovim<Compat<ChildStdin>>,
    io_handle: JoinHandle<Result<(), Box<LoopError>>>,
    child: tokio::process::Child,
}

impl NvimProcess {
    /// Returns a clone of the RPC handle for issuing generic calls.
    #[must_use]
    pub fn handle(&self) -> Neovim<Compat<ChildStdin>> {
        self.nvim.clone()
    }

    /// Attaches as an `ext_linegrid` + `ext_multigrid` UI at `cols * rows`.
    ///
    /// `ext_multigrid` makes Neovim emit per-window grids (one grid per editor
    /// window, plus the outer grid 1) and the window-layout events
    /// (`win_pos`/`win_viewport`/`win_close`/`grid_destroy`).
    pub async fn ui_attach(&self, cols: u16, rows: u16) -> Result<(), NvimError> {
        let opts = vec![
            (Value::from("ext_linegrid"), Value::from(true)),
            (Value::from("ext_multigrid"), Value::from(true)),
            (Value::from("rgb"), Value::from(true)),
        ];
        let _ = self
            .nvim
            .call(
                "nvim_ui_attach",
                vec![
                    Value::from(cols),
                    Value::from(rows),
                    Value::Map(opts),
                ],
            )
            .await
            .map_err(|e| NvimError::Attach(e.to_string()))?
            .map_err(|e| NvimError::Attach(format!("{e:?}")))?;
        Ok(())
    }

    /// Requests a UI resize via `nvim_ui_try_resize`. The authoritative grid size
    /// is whatever the resulting `grid_resize` carries.
    pub async fn try_resize(&self, cols: u16, rows: u16) -> Result<(), NvimError> {
        let _ = self
            .nvim
            .call(
                "nvim_ui_try_resize",
                vec![Value::from(cols), Value::from(rows)],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;
        Ok(())
    }

    /// Requests a per-grid resize via `nvim_ui_try_resize_grid(grid, cols, rows)`.
    ///
    /// Used for the multigrid path (resizing a single window grid); the
    /// authoritative size is whatever the resulting `grid_resize` carries.
    pub async fn try_resize_grid(&self, grid: u64, cols: u16, rows: u16) -> Result<(), NvimError> {
        let _ = self
            .nvim
            .call(
                "nvim_ui_try_resize_grid",
                vec![Value::from(grid), Value::from(cols), Value::from(rows)],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;
        Ok(())
    }

    /// Returns the captive nvim's current working directory via `getcwd()`.
    ///
    /// Calls `nvim_call_function("getcwd", [])` with NO arguments, so it honors
    /// the window/tab-local cwd of the current window (`:cd`/`:lcd`/`:tcd`). The
    /// result is extracted via [`Value::as_str`]; if it is missing or empty after
    /// trimming, returns [`NvimError::Rpc`] so the caller can fall back to a
    /// configured directory rather than rooting at an unusable path.
    pub async fn current_dir(&self) -> Result<PathBuf, NvimError> {
        let value = self
            .nvim
            .call(
                "nvim_call_function",
                vec![Value::from("getcwd"), Value::Array(vec![])],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;

        match value.as_str() {
            Some(s) if !s.trim().is_empty() => Ok(PathBuf::from(s)),
            _ => Err(NvimError::Rpc(format!(
                "getcwd returned no usable path: {value:?}"
            ))),
        }
    }

    /// Yanks the current visual-mode selection and returns its text.
    ///
    /// Runs ONE mode-guarded static Lua snippet via `nvim_exec_lua` (no string
    /// interpolation, so no injection hazard). The snippet returns `nil` unless
    /// the current mode is one of visual (`v`), visual-line (`V`), or
    /// visual-block (`<C-v>` = `"\22"`); otherwise it performs a synchronous
    /// yank (`nvim_feedkeys('y', 'nx', false)` — `x` drains typeahead so the
    /// yank completes before the call returns) and returns the unnamed register
    /// (`getreg('"')`), matching vim's visual-`y` semantics.
    ///
    /// The mode guard prevents a `y` operator-pending hang if this is invoked
    /// outside visual mode. `as_str` → `Some`, `Nil`/absent → `None`.
    pub async fn yank_visual_selection(&self) -> Result<Option<String>, NvimError> {
        const YANK_LUA: &str = "\
            local mode = vim.api.nvim_get_mode().mode\n\
            if mode ~= 'v' and mode ~= 'V' and mode ~= '\\22' then\n\
                return vim.NIL\n\
            end\n\
            vim.api.nvim_feedkeys('y', 'nx', false)\n\
            return vim.fn.getreg('\"')";

        let value = self
            .nvim
            .call(
                "nvim_exec_lua",
                vec![Value::from(YANK_LUA), Value::Array(vec![])],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;

        Ok(value.as_str().map(ToString::to_string))
    }

    /// Installs the current-file tracking autocmd inside the captive nvim.
    ///
    /// Looks up this RPC channel's id via `nvim_get_api_info` (which returns
    /// `[channel_id, metadata]`), then calls the `_G.wolf_setup_current_file_tracking`
    /// helper (defined in `runtime/nvim/init.lua`) over `nvim_exec_lua`, passing
    /// the channel id as a Lua vararg (`...`) rather than interpolating it into
    /// the code string, so there is no injection/formatting hazard. The helper
    /// registers a `BufEnter`/`BufWinEnter`/`BufFilePost` autocmd that
    /// `rpcnotify`s `wolf_current_file` and emits once immediately.
    pub async fn setup_current_file_tracking(&self) -> Result<(), NvimError> {
        let info = self
            .nvim
            .call("nvim_get_api_info", vec![])
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;

        let chan = info
            .as_array()
            .and_then(|a| a.first())
            .and_then(Value::as_i64)
            .ok_or_else(|| {
                NvimError::Rpc(format!(
                    "nvim_get_api_info returned no channel id: {info:?}"
                ))
            })?;

        let _ = self
            .nvim
            .call(
                "nvim_exec_lua",
                vec![
                    Value::from("wolf_setup_current_file_tracking(...)"),
                    Value::Array(vec![Value::from(chan)]),
                ],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;
        Ok(())
    }

    /// Installs the per-window buffer tracking autocmds inside the captive nvim.
    ///
    /// Mirrors [`setup_current_file_tracking`](Self::setup_current_file_tracking):
    /// looks up this RPC channel's id via `nvim_get_api_info`, then calls the
    /// `_G.wolf_setup_buffer_tracking` helper (defined in `runtime/nvim/init.lua`)
    /// over `nvim_exec_lua`, passing the channel id as a Lua vararg. The helper
    /// registers `BufEnter`/`BufWinEnter`/`BufFilePost`/`WinEnter`/`WinNew`/
    /// `WinClosed` autocmds that `rpcnotify` `wolf_buffers` with a list of
    /// `[win, nvim_buf_get_name(buf)]` pairs for ALL windows, and emits once
    /// immediately.
    pub async fn setup_buffer_tracking(&self) -> Result<(), NvimError> {
        let info = self
            .nvim
            .call("nvim_get_api_info", vec![])
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;

        let chan = info
            .as_array()
            .and_then(|a| a.first())
            .and_then(Value::as_i64)
            .ok_or_else(|| {
                NvimError::Rpc(format!(
                    "nvim_get_api_info returned no channel id: {info:?}"
                ))
            })?;

        let _ = self
            .nvim
            .call(
                "nvim_exec_lua",
                vec![
                    Value::from("wolf_setup_buffer_tracking(...)"),
                    Value::Array(vec![Value::from(chan)]),
                ],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;
        Ok(())
    }

    /// Installs the split-panes layout overrides inside the captive nvim.
    ///
    /// Mirrors [`setup_current_file_tracking`](Self::setup_current_file_tracking):
    /// looks up this RPC channel's id via `nvim_get_api_info`, then calls the
    /// `_G.wolf_setup_layout` helper (defined in `runtime/nvim/init.lua`) over
    /// `nvim_exec_lua`, passing the channel id as a Lua vararg. The helper rebinds
    /// the window split/close/focus chords and the `:split`/`:vsplit`/`:close`
    /// Ex-commands to `rpcnotify` `wolf_layout` payloads, and registers a
    /// `WinEnter` autocmd that `rpcnotify`s `wolf_active_win` (emitting once
    /// immediately).
    pub async fn setup_layout(&self) -> Result<(), NvimError> {
        let info = self
            .nvim
            .call("nvim_get_api_info", vec![])
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;

        let chan = info
            .as_array()
            .and_then(|a| a.first())
            .and_then(Value::as_i64)
            .ok_or_else(|| {
                NvimError::Rpc(format!(
                    "nvim_get_api_info returned no channel id: {info:?}"
                ))
            })?;

        let _ = self
            .nvim
            .call(
                "nvim_exec_lua",
                vec![
                    Value::from("wolf_setup_layout(...)"),
                    Value::Array(vec![Value::from(chan)]),
                ],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;
        Ok(())
    }

    /// Installs the directory-tracking autocmd inside the captive nvim.
    ///
    /// Mirrors [`setup_current_file_tracking`](Self::setup_current_file_tracking):
    /// looks up this RPC channel's id via `nvim_get_api_info`, then calls the
    /// `_G.wolf_setup_dir_tracking` helper (defined in `runtime/nvim/init.lua`)
    /// over `nvim_exec_lua`, passing the channel id as a Lua vararg. The helper
    /// registers a `DirChanged` autocmd (covering `:cd`/`:lcd`/`:tcd`) that
    /// `rpcnotify`s `wolf_cwd` with `getcwd()`, and emits once immediately so the
    /// startup cwd is reflected without a directory change.
    pub async fn setup_dir_tracking(&self) -> Result<(), NvimError> {
        let info = self
            .nvim
            .call("nvim_get_api_info", vec![])
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;

        let chan = info
            .as_array()
            .and_then(|a| a.first())
            .and_then(Value::as_i64)
            .ok_or_else(|| {
                NvimError::Rpc(format!(
                    "nvim_get_api_info returned no channel id: {info:?}"
                ))
            })?;

        let _ = self
            .nvim
            .call(
                "nvim_exec_lua",
                vec![
                    Value::from("wolf_setup_dir_tracking(...)"),
                    Value::Array(vec![Value::from(chan)]),
                ],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;
        Ok(())
    }

    /// Installs the theme-tracking autocmd inside the captive nvim.
    ///
    /// Mirrors [`setup_dir_tracking`](Self::setup_dir_tracking): looks up this
    /// RPC channel's id via `nvim_get_api_info`, then calls the
    /// `_G.wolf_setup_theme_tracking` helper (defined in
    /// `runtime/nvim/init.lua`) over `nvim_exec_lua`, passing the channel id as
    /// a Lua vararg. The helper registers a `ColorScheme` autocmd (covering
    /// `:colorscheme` and `:set background=...`) that `rpcnotify`s `wolf_theme`
    /// with the effective `WolfCursor` / `WolfTitlebar` colors, and emits once
    /// immediately so the startup colorscheme is reflected without a change.
    pub async fn setup_theme_tracking(&self) -> Result<(), NvimError> {
        let info = self
            .nvim
            .call("nvim_get_api_info", vec![])
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;

        let chan = info
            .as_array()
            .and_then(|a| a.first())
            .and_then(Value::as_i64)
            .ok_or_else(|| {
                NvimError::Rpc(format!(
                    "nvim_get_api_info returned no channel id: {info:?}"
                ))
            })?;

        let _ = self
            .nvim
            .call(
                "nvim_exec_lua",
                vec![
                    Value::from("wolf_setup_theme_tracking(...)"),
                    Value::Array(vec![Value::from(chan)]),
                ],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;
        Ok(())
    }

    /// Registers the Wolf command faces inside the captive nvim.
    ///
    /// Mirrors [`setup_theme_tracking`](Self::setup_theme_tracking): looks up this
    /// RPC channel's id via `nvim_get_api_info`, then calls the
    /// `_G.wolf_setup_commands` helper (defined in `runtime/nvim/init.lua`) over
    /// `nvim_exec_lua`, passing the channel id AND the command list as Lua varargs
    /// (never interpolated into the code string). The command list is built from
    /// [`COMMAND_SPECS`] as an array of `[id, ex_command]` string pairs — the
    /// single source of truth for both faces. The helper seeds `package.loaded['wolf']`
    /// and, per pair, registers a `require('wolf').<id>()` Lua function and a
    /// `:<ex_command>` user command, each `rpcnotify`ing `wolf_command '<id>'`.
    pub async fn setup_commands(&self) -> Result<(), NvimError> {
        let info = self
            .nvim
            .call("nvim_get_api_info", vec![])
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;

        let chan = info
            .as_array()
            .and_then(|a| a.first())
            .and_then(Value::as_i64)
            .ok_or_else(|| {
                NvimError::Rpc(format!(
                    "nvim_get_api_info returned no channel id: {info:?}"
                ))
            })?;

        let commands = Value::Array(
            COMMAND_SPECS
                .iter()
                .map(|spec| {
                    Value::Array(vec![Value::from(spec.id), Value::from(spec.ex_command)])
                })
                .collect(),
        );

        let _ = self
            .nvim
            .call(
                "nvim_exec_lua",
                vec![
                    Value::from("wolf_setup_commands(...)"),
                    Value::Array(vec![Value::from(chan), commands]),
                ],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;
        Ok(())
    }

    /// Cleanly shuts down: asks nvim to quit, awaits the io loop (bounded), then
    /// ensures the child is reaped.
    ///
    /// A front-end may hold a snapshot read guard across this call; that is safe
    /// because the redraw handler acquires the snapshot write lock cooperatively
    /// (see [`crate::render::SharedSnapshot::write_async`]) and so never pins a
    /// worker thread waiting for the reader to drop.
    pub async fn shutdown(mut self) -> Result<(), NvimError> {
        // Ask nvim to quit. nvim exits rather than replying, so bound the call and
        // ignore the result (a timeout/error just means it is already exiting).
        let _ = tokio::time::timeout(
            QUIT_GRACE,
            self.nvim.call("nvim_command", vec![Value::from("qa!")]),
        )
        .await;

        // Give the io loop a bounded chance to finish on its own (stdout EOF when
        // nvim exits); if it overruns, force-kill the child.
        match tokio::time::timeout(SHUTDOWN_TIMEOUT, &mut self.io_handle).await {
            Ok(_join) => {}
            Err(_elapsed) => {
                let _ = self.child.start_kill();
                self.io_handle.abort();
            }
        }
        // Best-effort reap so we leave no zombie.
        let _ = tokio::time::timeout(SHUTDOWN_TIMEOUT, self.child.wait()).await;
        Ok(())
    }
}

/// Spawns the captive Neovim with the given redraw handler.
///
/// Launches `--embed --clean -u <init.lua> -i NONE -n`, pinning `NVIM_APPNAME`
/// and the `XDG_*` dirs under `cfg.state_root`, and sets the child's working
/// directory to `cfg.cwd` (so its `getcwd()` starts there rather than inheriting
/// the host process's cwd). Returns the live process; the caller then
/// `ui_attach`es.
pub async fn spawn(cfg: &NvimConfig, handler: RedrawHandler) -> Result<NvimProcess, NvimError> {
    let config_home = cfg.state_root.join("config");
    let data_home = cfg.state_root.join("data");
    let state_home = cfg.state_root.join("state");
    let cache_home = cfg.state_root.join("cache");
    for d in [&config_home, &data_home, &state_home, &cache_home] {
        std::fs::create_dir_all(d).map_err(NvimError::StateDir)?;
    }

    let mut cmd = Command::new(&cfg.nvim_bin);
    cmd.arg("--embed")
        .arg("--clean")
        .arg("-u")
        .arg(&cfg.init_lua)
        .arg("-i")
        .arg("NONE")
        .arg("-n")
        .env("NVIM_APPNAME", &cfg.appname)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_DATA_HOME", &data_home)
        .env("XDG_STATE_HOME", &state_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    cmd.current_dir(&cfg.cwd);

    let (nvim, io_handle, child) = new_child_cmd(&mut cmd, handler)
        .await
        .map_err(NvimError::Spawn)?;

    Ok(NvimProcess {
        nvim,
        io_handle,
        child,
    })
}
