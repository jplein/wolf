//! The front-end-facing surface of `wolf-core`.
//!
//! [`WolfCore`] owns the tokio runtime, the captive Neovim process, and the
//! shared render-state snapshot. Its API is intentionally FFI-shaped (plain
//! structs, handles, a single wake callback) so the same surface serves both the
//! GTK front-end (direct crate) and the macOS front-end (via swift-bridge later).
//!
//! Output is a snapshot the front-end reads on its UI thread after a wake; input
//! is plain calls that core hands to the tokio side to issue RPC. There is one
//! wake callback, fired on `flush`; no per-event callbacks cross the boundary.

use std::path::PathBuf;
use std::time::Duration;

use tokio::runtime::Runtime;
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::input::{self, InputEvent, Nvim};
use crate::nvim::{self, NvimConfig, NvimError, NvimProcess};
use crate::redraw::{RedrawHandler, WakeFn};
use crate::render::{OwnedSnapshotReadGuard, SharedSnapshot, Snapshot};

/// How long the input pump waits after the first event of a burst for siblings
/// to enqueue, so a synchronous run of `send_input` calls coalesces into one
/// applied batch (one RPC for fused keys, hence one redraw flush).
const INPUT_COALESCE_DELAY: Duration = Duration::from_millis(8);

/// Configuration to start a [`WolfCore`].
#[derive(Clone, Debug)]
pub struct CoreConfig {
    /// Path to the in-repo `runtime/nvim/init.lua`.
    pub init_lua: PathBuf,
    /// Initial grid width in columns.
    pub cols: u16,
    /// Initial grid height in rows.
    pub rows: u16,
}

/// Errors from starting or driving the core.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// Failed to build the tokio runtime.
    #[error("failed to build tokio runtime: {0}")]
    Runtime(#[source] std::io::Error),
    /// An error from the captive Neovim layer.
    #[error(transparent)]
    Nvim(#[from] NvimError),
}

/// A read guard over the current render-state [`Snapshot`].
///
/// The front-end holds this for the duration of a paint, then drops it. It
/// `Deref`s to [`Snapshot`]. The guard owns an `Arc` clone of the underlying
/// lock, so its lifetime is independent of the [`WolfCore`] it came from.
pub struct SnapshotReadGuard {
    guard: OwnedSnapshotReadGuard,
}

impl std::ops::Deref for SnapshotReadGuard {
    type Target = Snapshot;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

/// The live core: tokio runtime + captive Neovim + shared snapshot.
pub struct WolfCore {
    runtime: Runtime,
    nvim: NvimProcess,
    snapshot: SharedSnapshot,
    /// Ordered input queue. Every input event is enqueued here and applied by a
    /// single consumer task in FIFO order, so keystrokes never reorder (which
    /// would garble typed text).
    input_tx: UnboundedSender<InputEvent>,
}

impl WolfCore {
    /// Starts the core: builds a multi-thread tokio runtime, spawns the captive
    /// Neovim, wires `wake` into the redraw handler, and attaches as an
    /// `ext_linegrid` UI at the configured size.
    pub fn start(cfg: CoreConfig, wake: WakeFn) -> Result<Self, CoreError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(CoreError::Runtime)?;

        let snapshot = SharedSnapshot::new(cfg.cols as usize, cfg.rows as usize);
        let handler = RedrawHandler::new(snapshot.clone_handle(), wake);

        let nvim_cfg = NvimConfig::from_env(cfg.init_lua, cfg.cols, cfg.rows)
            .map_err(CoreError::Nvim)?;

        let nvim = runtime.block_on(async {
            let proc = nvim::spawn(&nvim_cfg, handler).await?;
            proc.ui_attach(cfg.cols, cfg.rows).await?;
            Ok::<NvimProcess, NvimError>(proc)
        })?;

        // Spawn the single ordered input consumer. It owns its own RPC handle and
        // applies events strictly in FIFO order, so keystrokes never reorder.
        //
        // To match how a front-end delivers a burst (and how nvim coalesces its
        // redraws), the pump drains all currently-queued events before applying,
        // and fuses a run of consecutive `Key` events into one `nvim_input` call.
        // A burst of keys then produces a single RPC and a single redraw `flush`,
        // which is what callers that "send N keys, then wait for one flush" rely
        // on. A brief settle yield lets a synchronous burst fully enqueue first.
        let (input_tx, mut input_rx) = mpsc::unbounded_channel::<InputEvent>();
        let pump_nvim: Nvim = nvim.handle();
        runtime.spawn(async move {
            while let Some(first) = input_rx.recv().await {
                // Let any siblings enqueued in the same synchronous burst arrive.
                tokio::time::sleep(INPUT_COALESCE_DELAY).await;

                let mut batch = vec![first];
                while let Ok(ev) = input_rx.try_recv() {
                    batch.push(ev);
                }
                input::apply_batch(&pump_nvim, &batch).await;
            }
        });

        Ok(Self {
            runtime,
            nvim,
            snapshot,
            input_tx,
        })
    }

    /// Reads the current render-state snapshot (for painting on the UI thread).
    #[must_use]
    pub fn read_snapshot(&self) -> SnapshotReadGuard {
        SnapshotReadGuard {
            guard: self.snapshot.read_arc(),
        }
    }

    /// Forwards an input event to Neovim without blocking the caller.
    ///
    /// The event is enqueued on the ordered input channel and applied by the
    /// single consumer task in FIFO order; errors are logged (the front-end gets
    /// no synchronous result, matching the FFI-shaped contract).
    pub fn send_input(&self, event: InputEvent) {
        if self.input_tx.send(event).is_err() {
            tracing::warn!("input dropped: core input pump is gone");
        }
    }

    /// Requests a grid resize. The authoritative size arrives via `grid_resize`.
    /// Enqueued on the same ordered channel as input so it never races ahead of
    /// or behind pending keystrokes.
    pub fn resize(&self, cols: u16, rows: u16) {
        self.send_input(InputEvent::Resize { cols, rows });
    }

    /// Cleanly shuts down the captive Neovim and tears down the runtime.
    pub fn shutdown(self) -> Result<(), CoreError> {
        let WolfCore {
            runtime, nvim, ..
        } = self;
        runtime.block_on(async move { nvim.shutdown().await })?;
        Ok(())
    }
}
