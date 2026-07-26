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

use crate::buffer_picker::BufInfo;
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
    #[error(
        "WOLF_NVIM_BIN is not set; the pinned nvim binary must be provided by the flake devShell"
    )]
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
    /// A synchronous interactive RPC exceeded the configured timeout budget.
    #[error("nvim RPC '{0}' timed out after {1:?}")]
    Timeout(&'static str, Duration),
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

    /// Attaches as an `ext_linegrid` + `ext_multigrid` + `ext_popupmenu` +
    /// `ext_messages` UI at `cols * rows`.
    ///
    /// `ext_multigrid` makes Neovim emit per-window grids (one grid per editor
    /// window, plus the outer grid 1) and the window-layout events
    /// (`win_pos`/`win_viewport`/`win_close`/`grid_destroy`).
    ///
    /// `ext_popupmenu` routes the completion popup off the grid into
    /// `popupmenu_show`/`popupmenu_select`/`popupmenu_hide`. `ext_messages`
    /// routes messages off the grid into `msg_show`/`msg_clear`/
    /// `msg_history_show`; it IMPLICITLY enables `ext_cmdline` (the native command
    /// line, `cmdline_show`/`cmdline_pos`/`cmdline_hide`) and forces `cmdheight=0`
    /// — so the outer grid 1 empties (no grid-painted cmdline strip) and the
    /// front-ends paint the command line / messages natively from
    /// [`Snapshot::cmdline`](crate::render::Snapshot::cmdline) /
    /// [`Snapshot::toast`](crate::render::Snapshot::toast).
    pub async fn ui_attach(&self, cols: u16, rows: u16) -> Result<(), NvimError> {
        let opts = vec![
            (Value::from("ext_linegrid"), Value::from(true)),
            (Value::from("ext_multigrid"), Value::from(true)),
            (Value::from("ext_popupmenu"), Value::from(true)),
            (Value::from("ext_messages"), Value::from(true)),
            (Value::from("rgb"), Value::from(true)),
        ];
        let _ = self
            .nvim
            .call(
                "nvim_ui_attach",
                vec![Value::from(cols), Value::from(rows), Value::Map(opts)],
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

    /// Returns the absolute filesystem path of buffer `bufnr` via
    /// `nvim_buf_get_name`, or the EMPTY string for an unnamed (never-saved)
    /// buffer.
    ///
    /// Calls `nvim_buf_get_name([bufnr])` and reads the result via
    /// [`Value::as_str`]; nvim returns `""` for a buffer with no associated file.
    /// The browser-preview command ([`crate::api::WolfCore::open_in_browser`]) uses
    /// this to root the loopback static-site server at the pinned file's location.
    /// An RPC error is propagated as [`NvimError::Rpc`].
    pub async fn buffer_path(&self, bufnr: i64) -> Result<String, NvimError> {
        let value = self
            .nvim
            .call("nvim_buf_get_name", vec![Value::from(bufnr)])
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;

        Ok(value.as_str().unwrap_or_default().to_string())
    }

    /// Queries the captive nvim for its listed buffers, newest-used first-agnostic.
    ///
    /// Calls `nvim_call_function("getbufinfo", [ {buflisted=1} ])` — the single
    /// argument is an array holding ONE map `{"buflisted": 1}`, so only
    /// `buflisted` buffers are returned (regardless of whether they are loaded,
    /// e.g. a `:badd` buffer appears with `lastused` possibly 0). The result is an
    /// array of maps; each is decoded into a [`BufInfo`] by reading `bufnr` (i64),
    /// `name` (str), `lastused` (i64), and `changed` (i64 → `modified = changed !=
    /// 0`). Malformed entries (missing `bufnr`) are skipped defensively. The caller
    /// MRU-orders + ranks these via [`crate::buffer_picker::rank_buffers`].
    pub async fn list_buffers(&self) -> Result<Vec<BufInfo>, NvimError> {
        let opts = Value::Map(vec![(Value::from("buflisted"), Value::from(1))]);
        let value = self
            .nvim
            .call(
                "nvim_call_function",
                vec![Value::from("getbufinfo"), Value::Array(vec![opts])],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;

        let Some(entries) = value.as_array() else {
            return Err(NvimError::Rpc(format!(
                "getbufinfo did not return an array: {value:?}"
            )));
        };

        let mut buffers = Vec::with_capacity(entries.len());
        for entry in entries {
            let Value::Map(fields) = entry else { continue };
            let mut bufnr: Option<i64> = None;
            let mut name = String::new();
            let mut lastused: i64 = 0;
            let mut changed: i64 = 0;
            for (k, v) in fields {
                let Some(key) = k.as_str() else { continue };
                match key {
                    "bufnr" => bufnr = v.as_i64(),
                    "name" => {
                        if let Some(s) = v.as_str() {
                            name = s.to_string();
                        }
                    }
                    "lastused" => {
                        if let Some(n) = v.as_i64() {
                            lastused = n;
                        }
                    }
                    "changed" => {
                        if let Some(n) = v.as_i64() {
                            changed = n;
                        }
                    }
                    _ => {}
                }
            }
            let Some(bufnr) = bufnr else { continue };
            buffers.push(BufInfo {
                bufnr,
                name,
                lastused,
                modified: changed != 0,
            });
        }
        Ok(buffers)
    }

    /// Renders the Markdown SOURCE buffer `source_bufnr` to a themed HTML preview
    /// document.
    ///
    /// Blocks on `nvim_buf_get_lines(source_bufnr, 0, -1, false)` to read the whole
    /// buffer, joins the lines with `\n`, and renders via
    /// [`crate::markdown::render_html_with_highlights`] with the fixed
    /// [`crate::markdown::PreviewTheme::Default`] (OD5). Fenced/indented code blocks
    /// are treesitter-highlighted through the captive Neovim (see
    /// [`render_source_html`] and [`highlight_code_blocks`]); any block that cannot
    /// be highlighted falls back to plain `<pre><code>`. An RPC error (e.g. an
    /// unloaded/closed buffer) is propagated as [`NvimError::Rpc`]; the caller
    /// ([`crate::api::WolfCore::render_preview`]) maps that to an empty string.
    pub async fn render_preview(&self, source_bufnr: i64) -> Result<String, NvimError> {
        // In-app preview: full-bleed render (WidthMode::Full) — the webview FRAME
        // caps the content to the ~90-char width when the source is not full-width.
        render_source_html(&self.nvim, source_bufnr, crate::markdown::WidthMode::Full).await
    }

    /// Returns the number of the buffer currently being edited when it is a
    /// Markdown file, else `0`.
    ///
    /// Runs one static, mode-agnostic Lua snippet over `nvim_exec_lua` (no string
    /// interpolation): it returns `nvim_get_current_buf()` when the current
    /// buffer's `&filetype == "markdown"`, else `0`. The browser-preview command
    /// ([`crate::api::WolfCore::open_in_browser`]) uses this to PIN the source
    /// buffer the loopback server re-renders, gating on markdown the same way the
    /// in-app preview does. An RPC error is propagated as [`NvimError::Rpc`].
    pub async fn current_markdown_bufnr(&self) -> Result<i64, NvimError> {
        let value = self
            .nvim
            .call(
                "nvim_exec_lua",
                vec![
                    Value::from(
                        "return (vim.bo.filetype == 'markdown') and \
                         vim.api.nvim_get_current_buf() or 0",
                    ),
                    Value::Array(vec![]),
                ],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;

        Ok(value.as_i64().unwrap_or_default())
    }

    /// Opens (or reuses) the Markdown preview pane for the active source buffer and
    /// returns the preview scratch buffer's number (`0` when the active buffer is
    /// not Markdown).
    ///
    /// Runs the `_G.wolf_open_preview` helper (defined in `runtime/nvim/init.lua`)
    /// over `nvim_exec_lua`, passing `mode` as a Lua vararg (never interpolated).
    /// `mode` is `"split_right"` (single-pane placement: reuse `wolf_do_split('v')`
    /// then `nvim_win_set_buf(0, previewbuf)`), `"active"` (multi-pane: just
    /// `nvim_win_set_buf(0, previewbuf)`), or `"reuse_preview_pane"` (exactly two
    /// panes with the OTHER pane already a preview: scan the current tabpage for the
    /// other window carrying `b:wolf_preview_source` and bind into it, leaving the
    /// current window untouched; falls back to the `"active"` current-window bind
    /// when none) — see [`crate::api::WolfCore::open_preview`].
    /// The helper no-ops (returns `0`) unless `&filetype == "markdown"`, else
    /// creates-or-reuses the per-source scratch buffer and returns its bufnr,
    /// decoded via [`Value::as_i64`].
    pub async fn open_preview(&self, mode: &str) -> Result<i64, NvimError> {
        let value = self
            .nvim
            .call(
                "nvim_exec_lua",
                vec![
                    Value::from("return wolf_open_preview(...)"),
                    Value::Array(vec![Value::from(mode)]),
                ],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;

        Ok(value.as_i64().unwrap_or_default())
    }

    /// Navigates from the active PREVIEW pane back to its source file and returns
    /// the source buffer's number (`0` when the active buffer is not a preview).
    ///
    /// Runs the `_G.wolf_edit_source` helper (defined in `runtime/nvim/init.lua`)
    /// over `nvim_exec_lua`, passing `mode` as a Lua vararg (never interpolated).
    /// `mode` is `"split_left"` (single-pane placement: capture the original
    /// window, `wolf_do_split('v')`, bind the source into the original so the
    /// editor lands LEFT and the preview stays RIGHT), `"active"` (replace the
    /// preview in the current window with the source), or `"reuse_editor_pane"`
    /// (exactly two panes with the OTHER pane an editor: bind the source into that
    /// other window and focus it; falls back to the current window when none) — see
    /// [`crate::api::WolfCore::edit_source`]. The helper no-ops (returns `0`) unless
    /// the active buffer carries `b:wolf_preview_source`, else binds its source
    /// buffer and returns that source bufnr, decoded via [`Value::as_i64`].
    pub async fn edit_source(&self, mode: &str) -> Result<i64, NvimError> {
        let value = self
            .nvim
            .call(
                "nvim_exec_lua",
                vec![
                    Value::from("return wolf_edit_source(...)"),
                    Value::Array(vec![Value::from(mode)]),
                ],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;

        Ok(value.as_i64().unwrap_or_default())
    }

    /// Repoints the preview window `win` at the Markdown file at absolute `path`,
    /// replacing that pane's current preview with a preview of the linked file, and
    /// returns the linked file's SOURCE bufnr.
    ///
    /// Runs the `_G.wolf_preview_file` helper (defined in `runtime/nvim/init.lua`)
    /// over `nvim_exec_lua`, passing `win` and `path` as Lua varargs (never
    /// interpolated). The helper `bufadd`+`bufload`s `path`, gates it through
    /// `vim.filetype.match` (setting `filetype=markdown` when it matches), creates or
    /// reuses that source's preview scratch buffer via `_G.wolf_ensure_preview_buf`,
    /// binds it into `win` (falling back to the current window when `win` is
    /// missing/zero/invalid), and re-emits the previews / current-file snapshots.
    ///
    /// Returns the linked file's source bufnr (`> 0`) on success; `0` when `path` is
    /// empty, unreadable, or not Markdown (decoded via [`Value::as_i64`]). `path` is
    /// ABSOLUTE — the helper does NO relative resolution. RPC failure maps to
    /// [`NvimError::Rpc`]; the caller
    /// ([`crate::api::WolfCore::preview_file`]) maps that to `0`.
    pub async fn preview_file(&self, win: i64, path: &str) -> Result<i64, NvimError> {
        let value = self
            .nvim
            .call(
                "nvim_exec_lua",
                vec![
                    Value::from("return wolf_preview_file(...)"),
                    Value::Array(vec![Value::from(win), Value::from(path)]),
                ],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;

        Ok(value.as_i64().unwrap_or_default())
    }

    /// Installs the preview tracking + dirty-signal autocmds inside the captive
    /// nvim.
    ///
    /// Mirrors [`setup_buffer_tracking`](Self::setup_buffer_tracking): looks up this
    /// RPC channel's id via `nvim_get_api_info`, then calls the
    /// `_G.wolf_setup_preview` helper (defined in `runtime/nvim/init.lua`) over
    /// `nvim_exec_lua`, passing the channel id as a Lua vararg. The helper registers
    /// the `WolfPreview` augroup: a `WinEnter`/`BufWinEnter`/`WinNew`/`WinClosed`/
    /// `BufWinLeave` window-scan that `rpcnotify`s `wolf_preview` with
    /// `[preview_win, source_bufnr, source_abs_path]` triples (emitting once
    /// immediately), plus markdown-guarded `TextChanged`/`TextChangedI`/
    /// `BufWritePost` autocmds that `rpcnotify` `wolf_markdown_dirty` with
    /// `[source_bufnr, changedtick]`.
    pub async fn setup_preview(&self) -> Result<(), NvimError> {
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
                    Value::from("wolf_setup_preview(...)"),
                    Value::Array(vec![Value::from(chan)]),
                ],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;
        Ok(())
    }

    /// Creates a new nvim TABPAGE (optionally rooted at `cwd` via `:tcd`) and
    /// returns its stable HANDLE.
    ///
    /// Calls the `_G.wolf_create_tabpage` helper (defined in
    /// `runtime/nvim/init.lua`) over `nvim_exec_lua`, passing `cwd` as a Lua
    /// vararg (never interpolated). The helper runs `tabnew`, `:tcd` when `cwd`
    /// is non-empty, and returns `nvim_get_current_tabpage()`. The handle is
    /// decoded via [`crate::redraw::ext_handle`] (a plain integer, or defensively
    /// a msgpack-`Ext`-wrapped integer).
    pub async fn create_tabpage(&self, cwd: &str) -> Result<i64, NvimError> {
        let value = self
            .nvim
            .call(
                "nvim_exec_lua",
                vec![
                    Value::from("return wolf_create_tabpage(...)"),
                    Value::Array(vec![Value::from(cwd)]),
                ],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;

        crate::redraw::ext_handle(&value).ok_or_else(|| {
            NvimError::Rpc(format!("wolf_create_tabpage returned no handle: {value:?}"))
        })
    }

    /// Realizes a saved pane-split tree (`spec_json`) in the current tabpage's
    /// single starting window and returns the resulting window handles in DFS/leaf
    /// order.
    ///
    /// Calls the `_G.wolf_rebuild_layout` helper (defined in
    /// `runtime/nvim/init.lua`) over `nvim_exec_lua`, passing `spec_json` as a Lua
    /// vararg (never interpolated) that the helper `vim.json.decode`s. The helper
    /// creates the splits with `:split`/`:vsplit`, `:edit`s each non-empty leaf
    /// path, and returns the window handles in DFS order. Each handle is decoded
    /// via [`crate::redraw::ext_handle`].
    pub async fn rebuild_layout(&self, spec_json: &str) -> Result<Vec<i64>, NvimError> {
        let value = self
            .nvim
            .call(
                "nvim_exec_lua",
                vec![
                    Value::from("return wolf_rebuild_layout(...)"),
                    Value::Array(vec![Value::from(spec_json)]),
                ],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;

        let entries = value.as_array().ok_or_else(|| {
            NvimError::Rpc(format!("wolf_rebuild_layout returned non-array: {value:?}"))
        })?;
        entries
            .iter()
            .map(|v| {
                crate::redraw::ext_handle(v).ok_or_else(|| {
                    NvimError::Rpc(format!("wolf_rebuild_layout handle undecodable: {v:?}"))
                })
            })
            .collect()
    }

    /// Returns the current tabpage's stable HANDLE via `nvim_get_current_tabpage`.
    ///
    /// The authoritative bind-time value (the first-view bind reads it right after
    /// attach); decoded via [`crate::redraw::ext_handle`].
    pub async fn current_tabpage(&self) -> Result<i64, NvimError> {
        let value = self
            .nvim
            .call("nvim_get_current_tabpage", vec![])
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;

        crate::redraw::ext_handle(&value).ok_or_else(|| {
            NvimError::Rpc(format!(
                "nvim_get_current_tabpage returned no handle: {value:?}"
            ))
        })
    }

    /// Makes tabpage `handle` current via `nvim_set_current_tabpage`.
    pub async fn set_current_tabpage(&self, handle: i64) -> Result<(), NvimError> {
        let _ = self
            .nvim
            .call("nvim_set_current_tabpage", vec![Value::from(handle)])
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;
        Ok(())
    }

    /// Closes tabpage `handle` via the `_G.wolf_close_tabpage` helper.
    ///
    /// The helper resolves the STABLE handle to its tabpage NUMBER and runs
    /// `<n>tabclose` inside a `pcall` (a last-tabpage `E784` is swallowed).
    pub async fn close_tabpage(&self, handle: i64) -> Result<(), NvimError> {
        let _ = self
            .nvim
            .call(
                "nvim_exec_lua",
                vec![
                    Value::from("wolf_close_tabpage(...)"),
                    Value::Array(vec![Value::from(handle)]),
                ],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;
        Ok(())
    }

    /// Installs the tabpage tracking + nav/creation lockout inside the captive
    /// nvim.
    ///
    /// Mirrors [`setup_dir_tracking`](Self::setup_dir_tracking): looks up this RPC
    /// channel's id via `nvim_get_api_info`, then calls the `_G.wolf_setup_tabs`
    /// helper (defined in `runtime/nvim/init.lua`) over `nvim_exec_lua`, passing
    /// the channel id as a Lua vararg. The helper registers `TabEnter` /
    /// `TabNewEntered` autocmds that `rpcnotify` `wolf_tabpage` with
    /// `nvim_get_current_tabpage()` (emitting once immediately), a second
    /// `DirChanged`/`TabNewEntered`/`TabClosed` autocmd that `rpcnotify`s
    /// `wolf_tab_cwds` with every tabpage's `[handle, getcwd(-1, n)]` (also
    /// emitting once immediately), locks out the nvim-internal tabpage
    /// navigation/creation chords + typed `:tab*` Ex-commands, and defines the
    /// `wolf_create_tabpage` / `wolf_close_tabpage` helpers.
    pub async fn setup_tabs(&self) -> Result<(), NvimError> {
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
                    Value::from("wolf_setup_tabs(...)"),
                    Value::Array(vec![Value::from(chan)]),
                ],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;
        Ok(())
    }

    /// Installs the per-pane view-tracking autocmds inside the captive nvim.
    ///
    /// Unlike the other `setup_*` helpers, this passes NO channel id: the
    /// `_G.wolf_setup_view_tracking` helper (defined in `runtime/nvim/init.lua`)
    /// emits no `rpcnotify` — it only maintains an in-nvim `(winid, bufnr) ->
    /// winsaveview()` cache (via `BufLeave`/`WinLeave`/`WinClosed` autocmds) and
    /// defines the `_G.wolf_switch_buffer` helper that reads it. So this is a bare
    /// `nvim_exec_lua("wolf_setup_view_tracking()", [])` with no payload.
    pub async fn setup_view_tracking(&self) -> Result<(), NvimError> {
        let _ = self
            .nvim
            .call(
                "nvim_exec_lua",
                vec![
                    Value::from("wolf_setup_view_tracking()"),
                    Value::Array(vec![]),
                ],
            )
            .await
            .map_err(|e| NvimError::Rpc(e.to_string()))?
            .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;
        Ok(())
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
    /// registers a `BufEnter`/`BufWinEnter`/`BufFilePost`/`FileType` autocmd that
    /// `rpcnotify`s `wolf_current_file` with TWO args — the active buffer's
    /// absolute path (`expand('%:p')`) AND its filetype (`vim.bo.filetype`, which
    /// drives [`crate::render::Snapshot::is_markdown_active`]) — and emits once
    /// immediately. `FileType` is in the event set so the filetype tracks the
    /// active buffer even when detection lands after the buffer switch. A preview
    /// scratch buffer (carrying `b:wolf_preview_source`) is SKIPPED: it emits an
    /// EMPTY path and EMPTY filetype, so core never sees the `Preview: <name>`
    /// scratch name as a real current file and never reports markdown-active for
    /// the (non-markdown) preview pane.
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

    /// Installs the per-window full-width tracking autocmds inside the captive nvim.
    ///
    /// Mirrors [`setup_buffer_tracking`](Self::setup_buffer_tracking): looks up this
    /// RPC channel's id via `nvim_get_api_info`, then calls the
    /// `_G.wolf_setup_full_width` helper (defined in `runtime/nvim/init.lua`) over
    /// `nvim_exec_lua`, passing the channel id as a Lua vararg. The helper registers
    /// `BufEnter`/`BufWinEnter`/`WinEnter`/`WinNew`/`WinClosed` autocmds that
    /// `rpcnotify` `wolf_full_width` with a list of
    /// `[win, vim.b[buf].wolf_full_width == true]` pairs for ALL windows, and emits
    /// once immediately.
    pub async fn setup_full_width(&self) -> Result<(), NvimError> {
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
                    Value::from("wolf_setup_full_width(...)"),
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
                .map(|spec| Value::Array(vec![Value::from(spec.id), Value::from(spec.ex_command)]))
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

/// Renders the Markdown SOURCE buffer `source_bufnr` to a themed HTML preview
/// document over the RPC handle `nvim`.
///
/// Blocks on `nvim_buf_get_lines(source_bufnr, 0, -1, false)` to read the whole
/// buffer, joins the lines with `\n`, enumerates its code blocks
/// ([`crate::markdown::code_blocks`]), asks the captive Neovim to
/// treesitter-highlight them ([`highlight_code_blocks`]), and renders via
/// [`crate::markdown::render_html_with_highlights`] with the fixed
/// [`crate::markdown::PreviewTheme::Default`] (OD5) and the given `width_mode`
/// (in-app callers pass [`crate::markdown::WidthMode::Full`] and let the webview
/// frame cap; the browser preview passes [`crate::markdown::WidthMode::Capped`]).
/// The highlight pass degrades gracefully: any block Neovim cannot highlight
/// (unknown language, missing parser/query, or the no-op treesitter path on
/// Linux/cargo) renders as plain `<pre><code>`. An RPC error (e.g. an
/// unloaded/closed buffer) reading the buffer is propagated as [`NvimError::Rpc`].
///
/// Extracted from [`NvimProcess::render_preview`] so the loopback browser-preview
/// server ([`crate::browser_preview`]) can re-render off a bare RPC handle
/// ([`NvimProcess::handle`]) without borrowing the whole [`NvimProcess`].
pub(crate) async fn render_source_html(
    nvim: &Neovim<Compat<ChildStdin>>,
    source_bufnr: i64,
    width_mode: crate::markdown::WidthMode,
) -> Result<String, NvimError> {
    let value = nvim
        .call(
            "nvim_buf_get_lines",
            vec![
                Value::from(source_bufnr),
                Value::from(0),
                Value::from(-1),
                Value::from(false),
            ],
        )
        .await
        .map_err(|e| NvimError::Rpc(e.to_string()))?
        .map_err(|e| NvimError::Rpc(format!("{e:?}")))?;

    let Some(lines) = value.as_array() else {
        return Err(NvimError::Rpc(format!(
            "nvim_buf_get_lines did not return an array: {value:?}"
        )));
    };
    let source = lines
        .iter()
        .map(|l| l.as_str().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");

    // Enumerate the fenced/indented code blocks and ask the captive Neovim's
    // treesitter to highlight each. ANY failure (RPC error, no parser/query,
    // Linux/cargo no-op) degrades to `None` per block, i.e. the plain render.
    let blocks = crate::markdown::code_blocks(&source);
    let highlights = if blocks.is_empty() {
        Vec::new()
    } else {
        highlight_code_blocks(nvim, &blocks).await
    };

    Ok(crate::markdown::render_html_with_highlights(
        &source,
        crate::markdown::PreviewTheme::Default,
        width_mode,
        &highlights,
    ))
}

/// Asks the captive Neovim to treesitter-highlight a BATCH of code blocks in one
/// RPC round-trip, returning per-block highlight data aligned to `blocks` by index.
///
/// Calls the module-level Lua helper `wolf_highlight_code_blocks(blocks)` (see
/// `runtime/nvim/init.lua`) with an array of `[text, lang]` pairs. The reply is an
/// array whose i-th entry is either `false`/nil (not highlightable — unknown
/// language, missing parser/query, or the whole treesitter block a no-op as on
/// Linux/cargo) or an array of 7-element span tuples
/// `[row, start_col, end_col, fg, bold, italic, underline]`.
///
/// This is TOTAL graceful fallback: any RPC error, a non-array reply, or a
/// length mismatch yields `vec![None; blocks.len()]` (every block plain), and any
/// individual malformed entry decodes to `None`. It never panics and never
/// unwraps on the decode path.
async fn highlight_code_blocks(
    nvim: &Neovim<Compat<ChildStdin>>,
    blocks: &[crate::markdown::CodeBlockInfo],
) -> Vec<Option<crate::markdown::BlockHighlight>> {
    let fallback = || vec![None; blocks.len()];

    let lua_blocks: Vec<Value> = blocks
        .iter()
        .map(|b| {
            Value::Array(vec![
                Value::from(b.text.clone()),
                Value::from(b.lang.as_deref().unwrap_or("")),
            ])
        })
        .collect();

    let reply = nvim
        .call(
            "nvim_exec_lua",
            vec![
                Value::from("return wolf_highlight_code_blocks(...)"),
                Value::Array(vec![Value::Array(lua_blocks)]),
            ],
        )
        .await;

    let Ok(Ok(value)) = reply else {
        return fallback();
    };
    let Some(results) = value.as_array() else {
        return fallback();
    };
    if results.len() != blocks.len() {
        return fallback();
    }

    results.iter().map(decode_block_highlight).collect()
}

/// Decodes one per-block reply entry from `wolf_highlight_code_blocks` into an
/// `Option<BlockHighlight>`.
///
/// `Boolean(false)`/`Nil` → `None` (not highlightable). An array of 7-element span
/// tuples decodes each into a [`crate::markdown::HighlightSpan`]; a `fg` of `< 0`
/// means "no foreground" (`color: None`). Anything else (wrong shape) → `None`.
fn decode_block_highlight(entry: &Value) -> Option<crate::markdown::BlockHighlight> {
    let spans_val = entry.as_array()?;
    let mut spans = Vec::with_capacity(spans_val.len());
    for span in spans_val {
        let fields = span.as_array()?;
        if fields.len() < 7 {
            return None;
        }
        let row = fields.first().and_then(Value::as_i64)? as u32;
        let start_col = fields.get(1).and_then(Value::as_i64)? as u32;
        let end_col = fields.get(2).and_then(Value::as_i64)? as u32;
        let fg = fields.get(3).and_then(Value::as_i64)?;
        let bold = value_is_true(fields.get(4));
        let italic = value_is_true(fields.get(5));
        let underline = value_is_true(fields.get(6));
        let color = if fg < 0 { None } else { Some(fg as u32) };
        spans.push(crate::markdown::HighlightSpan {
            row,
            start_col,
            end_col,
            style: crate::markdown::HighlightStyle {
                color,
                bold,
                italic,
                underline,
            },
        });
    }
    Some(crate::markdown::BlockHighlight { spans })
}

/// Interprets an optional msgpack [`Value`] as a boolean flag, tolerating either a
/// real `Boolean` or an integer (`0` = false, non-zero = true). Missing/other → false.
fn value_is_true(value: Option<&Value>) -> bool {
    match value {
        Some(v) => v
            .as_bool()
            .unwrap_or_else(|| v.as_i64().is_some_and(|n| n != 0)),
        None => false,
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
