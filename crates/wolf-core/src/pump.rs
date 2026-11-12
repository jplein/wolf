//! The input pump: the single consumer task of the ordered input channel that
//! [`WolfCore::send_input`](crate::WolfCore::send_input) feeds.
//!
//! It applies events in batches through [`input::apply_batch`], and keeps taking input while one
//! of a batch's calls waits on Neovim, so the key that answers a prompt the call raised can reach
//! Neovim past it.

use std::collections::VecDeque;
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::AtomicU8;
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;

use parking_lot::RwLock;
use rmpv::Value;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::{JoinError, JoinHandle};

use crate::input::{self, CallResult, InputEvent, Nvim, Rpc};
use crate::liveness;
use crate::mru::MruStore;
use crate::redraw::{TrackedCwd, WakeFn};
use crate::render::{SharedSnapshot, Snapshot};

/// How long the input pump waits after the first event of a burst for siblings
/// to enqueue, so a synchronous run of `send_input` calls coalesces into one
/// applied batch (one RPC for fused keys, hence one redraw flush).
pub(crate) const INPUT_COALESCE_DELAY: Duration = Duration::from_millis(8);

/// The handle the input pump applies batches to, and the restart generation it
/// belongs to. [`WolfCore::restart_nvim`](crate::WolfCore::restart_nvim) swaps both in one
/// write-lock section.
#[derive(Clone)]
pub(crate) struct PumpTarget {
    pub(crate) nvim: Nvim,
    pub(crate) generation: u64,
}

/// An input event as queued by [`WolfCore::send_input`](crate::WolfCore::send_input), stamped
/// with the restart generation current when it was sent.
pub(crate) struct QueuedInput {
    pub(crate) generation: u64,
    pub(crate) event: InputEvent,
}

/// The pump's state. [`WolfCore::start`](crate::WolfCore::start) builds it from a struct literal
/// and spawns [`run`](Self::run).
pub(crate) struct Pump {
    /// The receiving end of the input channel.
    pub(crate) rx: UnboundedReceiver<QueuedInput>,
    /// The current handle and generation, which a restart swaps.
    pub(crate) shared_handle: Arc<RwLock<PumpTarget>>,
    /// The global recent-files store, which records every `Open` and `OpenAt`.
    pub(crate) mru: MruStore,
    /// Neovim's live cwd, the directory an open is recorded under.
    pub(crate) tracked_cwd: TrackedCwd,
    /// The directory an open is recorded under before Neovim first reports its cwd.
    pub(crate) fallback_cwd: PathBuf,
    /// The render snapshot, for the toast and the prompt.
    pub(crate) snapshot: SharedSnapshot,
    /// The captive Neovim's liveness cell.
    pub(crate) liveness: Arc<AtomicU8>,
    /// The front-end wake, fired on a liveness transition.
    pub(crate) wake: WakeFn,
    /// How long a call may stay unanswered before the pump calls it a stall.
    pub(crate) rpc_timeout: Duration,
}

impl Pump {
    /// Runs until the input channel closes and every event received is applied.
    ///
    /// A batch is the run of fast inputs at the front of the queue plus the one event after it,
    /// so it holds at most one event that is not a fast input, and that event comes last. Every
    /// event behind it waits in the queue, in arrival order, while its calls are driven.
    pub(crate) async fn run(mut self) {
        let mut queue: VecDeque<QueuedInput> = VecDeque::new();
        let mut closed = false;
        loop {
            if queue.is_empty() {
                let Some(first) = self.rx.recv().await else {
                    return;
                };
                queue.push_back(first);
                // Let any siblings enqueued in the same synchronous burst arrive.
                tokio::time::sleep(INPUT_COALESCE_DELAY).await;
            }
            while let Ok(ev) = self.rx.try_recv() {
                queue.push_back(ev);
            }

            let fast_run = queue.iter().take_while(|q| q.event.is_fast_input()).count();
            let take = (fast_run + 1).min(queue.len());
            let queued: Vec<QueuedInput> = queue.drain(..take).collect();

            // Read the CURRENT handle for this batch (survives a restart swap).
            let target: PumpTarget = self.shared_handle.read().clone();

            // Typing sent before the last restart's swap is dropped, so a half-typed `:` cannot
            // reach the fresh nvim as normal-mode keys; state input still applies.
            let queued_len = queued.len();
            let batch: Vec<InputEvent> = queued
                .into_iter()
                .filter(|q| q.generation == target.generation || !q.event.is_typing())
                .map(|q| q.event)
                .collect();
            if batch.len() < queued_len {
                tracing::debug!(
                    dropped = queued_len - batch.len(),
                    "dropped typing queued before a restart"
                );
            }
            if batch.is_empty() {
                continue;
            }

            if batch.iter().any(InputEvent::is_toast_dismissing) {
                clear_dismissable_toast(&self.snapshot).await;
            }
            let entering = {
                let snap = self.snapshot.read();
                batch.iter().any(|event| enters_terminal(event, &snap))
            };
            if entering {
                dismiss_toast_and_wake(&self.snapshot, &self.wake).await;
            }

            let mut driver = Driver {
                target: &target,
                rx: &mut self.rx,
                queue: &mut queue,
                closed: &mut closed,
                shared_handle: &self.shared_handle,
                snapshot: &self.snapshot,
                liveness: &self.liveness,
                wake: &self.wake,
                rpc_timeout: self.rpc_timeout,
            };
            let answered = input::apply_batch(&mut driver, &batch).await;
            if answered && self.shared_handle.read().generation == target.generation {
                liveness::clear_not_responding(&self.liveness, &self.wake);
            }

            // Record opens under nvim's live cwd, resolved per batch to follow `:cd`/`:tcd`;
            // the path is cloned out so the guard is not held across the record.
            for ev in &batch {
                if let InputEvent::Open { path } | InputEvent::OpenAt { path, .. } = ev {
                    let record_cwd = self
                        .tracked_cwd
                        .read()
                        .clone()
                        .unwrap_or_else(|| self.fallback_cwd.clone());
                    self.mru.record(&record_cwd, path);
                }
            }
        }
    }
}

/// Clears a toast that is not a prompt, before dismissing input applies, so a `msg_show` that
/// input produces repopulates it. The guard drops before the caller's next `.await`.
async fn clear_dismissable_toast(snapshot: &SharedSnapshot) {
    let mut snap = snapshot.write_async().await;
    if !snap.toast.is_prompt() {
        snap.clear_toast();
    }
}

/// Dismisses a toast that is not a `confirm` prompt, keeping a stashed confirm, and fires the
/// wake when it hid one, since no redraw flush need follow. It leaves `seq` alone.
pub(crate) async fn dismiss_toast_and_wake(snapshot: &SharedSnapshot, wake: &WakeFn) {
    let dismissed = snapshot.write_async().await.dismiss_toast();
    if dismissed {
        wake();
    }
}

/// Whether `event` takes the user into a terminal pane: a drawer toggle, either way, or a
/// `FocusWin` into a window that shows a session.
fn enters_terminal(event: &InputEvent, snap: &Snapshot) -> bool {
    match event {
        InputEvent::ToggleTerminal { .. } => true,
        InputEvent::FocusWin { win } => snap
            .win_buffers
            .get(win)
            .is_some_and(|buffer| buffer.terminal.is_some()),
        _ => false,
    }
}

/// Whether Neovim is showing a prompt that waits on the user: a confirm toast, or an active
/// command line that carries a prompt string. A plain `:` command line has an empty prompt.
pub(crate) fn prompt_showing(snap: &Snapshot) -> bool {
    snap.toast.is_prompt() || (snap.cmdline.active && !snap.cmdline.prompt.is_empty())
}

/// The index in `queue` of the event a probe answer may release: the oldest fast input of
/// `generation`, past any state event or paste held ahead of it. Typing of another generation is
/// stale and skipped, and state events are never released.
///
/// A paste is never released: its answer waits on the call, and as the release in flight it
/// would hold back every key behind it. A paste never stops a release either: the key that ends
/// Neovim's wait would then wait on the paste, which waits on the call.
fn releasable(queue: &VecDeque<QueuedInput>, generation: u64) -> Option<usize> {
    queue
        .iter()
        .position(|q| q.generation == generation && q.event.is_fast_input())
}

/// Awaits the task in `slot`, or never resolves when there is none. `tokio::select!` builds the
/// future of a disabled branch too, so the branch over a slot must not unwrap it.
async fn join_slot<T>(slot: &mut Option<JoinHandle<T>>) -> Result<T, JoinError> {
    match slot {
        Some(handle) => handle.await,
        None => std::future::pending().await,
    }
}

/// The [`Rpc`] one batch is applied through. While a call is unanswered it keeps receiving input,
/// and releases held fast input to a Neovim that waits on a keystroke.
///
/// See notes/wolf-core-decisions.md#threading-and-locking
struct Driver<'a> {
    target: &'a PumpTarget,
    rx: &'a mut UnboundedReceiver<QueuedInput>,
    queue: &'a mut VecDeque<QueuedInput>,
    closed: &'a mut bool,
    shared_handle: &'a RwLock<PumpTarget>,
    snapshot: &'a SharedSnapshot,
    liveness: &'a Arc<AtomicU8>,
    wake: &'a WakeFn,
    rpc_timeout: Duration,
}

impl Driver<'_> {
    /// Whether the batch's handle is still the current one, so its stall or answer counts.
    fn current(&self) -> bool {
        self.shared_handle.read().generation == self.target.generation
    }

    /// Takes the releasable event out of the queue and spawns its apply, or returns `None` when
    /// nothing is releasable.
    fn release(&mut self) -> Option<JoinHandle<()>> {
        let index = releasable(self.queue, self.target.generation)?;
        let event = self.queue.remove(index)?.event;
        let mut nvim = self.target.nvim.clone();
        let snapshot = self.snapshot.clone_handle();
        Some(tokio::spawn(async move {
            if event.is_toast_dismissing() {
                clear_dismissable_toast(&snapshot).await;
            }
            if let Err(e) = input::apply(&mut nvim, &event).await {
                tracing::warn!("released input apply failed: {e}");
            }
        }))
    }
}

impl Rpc for Driver<'_> {
    /// Issues the call, and drives it to its answer while taking input.
    ///
    /// The call is pinned and never dropped, since dropping a pending nvim-rs call kills the io
    /// loop. Probes and releases are spawned tasks, which a dropped `JoinHandle` only detaches.
    async fn request(&mut self, method: &str, args: Vec<Value>) -> CallResult {
        let nvim = self.target.nvim.clone();
        let mut call = std::pin::pin!(nvim.call(method, args));
        let budget = tokio::time::sleep(self.rpc_timeout);
        let mut budget = std::pin::pin!(budget);
        let mut probe: Option<JoinHandle<CallResult>> = None;
        let mut in_flight: Option<JoinHandle<()>> = None;
        let mut probing = true;

        // Poll once before any probe: the first poll writes the request unless another writer
        // holds the pipe, so a probe follows the call to Neovim.
        let mut answer = match std::future::poll_fn(|cx| Poll::Ready(call.as_mut().poll(cx))).await
        {
            Poll::Ready(result) => Some(result),
            Poll::Pending => None,
        };

        loop {
            if in_flight.is_none() {
                if let Some(result) = answer.take() {
                    return result;
                }
            }
            if answer.is_none()
                && probing
                && probe.is_none()
                && in_flight.is_none()
                && releasable(self.queue, self.target.generation).is_some()
            {
                let nvim = self.target.nvim.clone();
                probe = Some(tokio::spawn(async move {
                    nvim.call("nvim_get_mode", Vec::new()).await
                }));
            }

            tokio::select! {
                biased;
                result = &mut call, if answer.is_none() => answer = Some(result),
                _ = join_slot(&mut in_flight), if in_flight.is_some() => in_flight = None,
                result = join_slot(&mut probe), if probe.is_some() && answer.is_none() => {
                    probe = None;
                    if let Ok(Ok(_)) = result {
                        // Answered before the call, so Neovim waits for input.
                        if self.current() {
                            liveness::clear_not_responding(self.liveness, self.wake);
                        }
                        in_flight = self.release();
                    } else {
                        // A transport failure answers the call too; stop probing until it does.
                        probing = false;
                    }
                }
                received = self.rx.recv(), if !*self.closed => match received {
                    Some(q) => self.queue.push_back(q),
                    None => *self.closed = true,
                },
                () = &mut budget, if answer.is_none() => {
                    let showing = prompt_showing(&self.snapshot.read());
                    if self.current() {
                        if showing {
                            liveness::clear_not_responding(self.liveness, self.wake);
                        } else {
                            liveness::mark_not_responding(self.liveness, self.wake);
                        }
                    }
                    budget
                        .as_mut()
                        .reset(tokio::time::Instant::now() + self.rpc_timeout);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::{prompt_showing, releasable, QueuedInput};
    use crate::input::{InputEvent, Key, KeyEvent, ScrollDirection, ScrollEvent};
    use crate::render::{Message, Snapshot};

    fn key(c: char) -> InputEvent {
        InputEvent::Key(KeyEvent {
            key: Key::Char(c),
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        })
    }

    fn queued(generation: u64, event: InputEvent) -> QueuedInput {
        QueuedInput { generation, event }
    }

    fn queue(events: Vec<QueuedInput>) -> VecDeque<QueuedInput> {
        events.into_iter().collect()
    }

    #[test]
    fn a_confirm_toast_is_a_prompt() {
        let mut snap = Snapshot::new(80, 24);
        snap.toast.messages.push(Message {
            kind: "confirm".to_string(),
            ..Message::default()
        });
        assert!(prompt_showing(&snap));
    }

    #[test]
    fn an_active_cmdline_with_a_prompt_is_a_prompt() {
        let mut snap = Snapshot::new(80, 24);
        snap.cmdline.active = true;
        snap.cmdline.prompt = "Do you really want to write to it (y/n)?".to_string();
        assert!(prompt_showing(&snap));
    }

    #[test]
    fn an_active_colon_cmdline_is_not_a_prompt() {
        let mut snap = Snapshot::new(80, 24);
        snap.cmdline.active = true;
        snap.cmdline.firstc = ":".to_string();
        assert!(!prompt_showing(&snap));
    }

    #[test]
    fn an_inactive_cmdline_is_not_a_prompt() {
        let mut snap = Snapshot::new(80, 24);
        snap.cmdline.prompt = "(y/n)".to_string();
        assert!(!prompt_showing(&snap));
    }

    #[test]
    fn release_takes_the_oldest_fast_typing_past_a_state_event() {
        let q = queue(vec![
            queued(3, InputEvent::SetBackground { dark: true }),
            queued(
                3,
                InputEvent::Scroll(ScrollEvent {
                    direction: ScrollDirection::Down,
                    grid: 2,
                    row: 0,
                    col: 0,
                }),
            ),
            queued(3, key('y')),
        ]);
        assert_eq!(releasable(&q, 3), Some(1));
    }

    #[test]
    fn release_passes_a_held_paste() {
        let paste = || {
            queued(
                3,
                InputEvent::Paste {
                    text: "pasted".to_string(),
                },
            )
        };
        assert_eq!(
            releasable(&queue(vec![paste(), queued(3, key('y'))]), 3),
            Some(1)
        );
        assert_eq!(releasable(&queue(vec![paste()]), 3), None);
    }

    #[test]
    fn stale_typing_is_skipped_and_left_queued() {
        let q = queue(vec![queued(2, key('x')), queued(3, key('y'))]);
        assert_eq!(releasable(&q, 3), Some(1));
        assert_eq!(q.len(), 2, "selection must not take the stale key out");
        assert_eq!(releasable(&queue(vec![queued(2, key('x'))]), 3), None);
    }
}
