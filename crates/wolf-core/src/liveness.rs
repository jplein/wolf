//! Liveness state model for the captive Neovim.
//!
//! A single, lock-free `Alive` / `NotResponding` / `Dead` state the supervisor
//! (crash detection) and `block_on_rpc`'s timeout path (stall detection) write
//! from tokio worker threads, and the FFI `liveness` accessor reads from the
//! AppKit main thread. Backed by an `Arc<std::sync::atomic::AtomicU8>` so it
//! never contends the snapshot lock, mutated ONLY through the transition helpers
//! below, each of which fires the `WakeFn` exactly ONCE per real state change so
//! the front-end can re-read `liveness()` and paint/recover.

use std::sync::atomic::{AtomicU8, Ordering};

use crate::redraw::WakeFn;

/// The captive Neovim's liveness state, written by the supervisor (crash
/// detection) and `block_on_rpc`'s timeout path (stall detection) and read by the
/// FFI accessor. Backed by an [`AtomicU8`] via the free transition helpers below.
///
/// The discriminants are STABLE (`0 / 1 / 2`): the FFI `liveness` module mirrors
/// them as its `ALIVE` / `NOT_RESPONDING` / `DEAD` constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    /// The captive nvim is responding normally.
    Alive,
    /// The captive nvim has not answered a synchronous RPC within the timeout
    /// budget (a stall). Recoverable: a subsequent successful RPC clears it back
    /// to `Alive`, or the user drives a manual restart.
    NotResponding,
    /// The captive nvim process has died (stdout EOF, clean or crash). TERMINAL:
    /// only [`reset_alive`] (the restart path) leaves this state.
    Dead,
}

impl Liveness {
    /// The FFI-mirror discriminant (`0` / `1` / `2`).
    #[must_use]
    pub fn to_u8(self) -> u8 {
        match self {
            Liveness::Alive => 0,
            Liveness::NotResponding => 1,
            Liveness::Dead => 2,
        }
    }

    /// The inverse of [`to_u8`](Self::to_u8). Any unknown value clamps to `Dead`
    /// (the safe, terminal state — an unrecognized byte must never read as
    /// `Alive`).
    #[must_use]
    pub fn from_u8(v: u8) -> Liveness {
        match v {
            0 => Liveness::Alive,
            1 => Liveness::NotResponding,
            _ => Liveness::Dead,
        }
    }
}

/// Reads the current [`Liveness`] out of the atomic cell.
#[must_use]
pub fn get(state: &AtomicU8) -> Liveness {
    Liveness::from_u8(state.load(Ordering::SeqCst))
}

/// Marks the captive nvim `Dead` (unconditional store). Fires `wake` iff the
/// state was NOT already `Dead`. `Dead` is TERMINAL: only [`reset_alive`] leaves
/// it (the restart path).
pub fn mark_dead(state: &AtomicU8, wake: &WakeFn) {
    let previous = state.swap(Liveness::Dead.to_u8(), Ordering::SeqCst);
    if previous != Liveness::Dead.to_u8() {
        wake();
    }
}

/// Escalates a stall: `compare_exchange(Alive -> NotResponding)` ONLY. Never
/// downgrades `Dead` (a late stall signal after death is a no-op), and never
/// re-fires when already `NotResponding`. Fires `wake` only on a successful
/// transition.
pub fn mark_not_responding(state: &AtomicU8, wake: &WakeFn) {
    if state
        .compare_exchange(
            Liveness::Alive.to_u8(),
            Liveness::NotResponding.to_u8(),
            Ordering::SeqCst,
            Ordering::SeqCst,
        )
        .is_ok()
    {
        wake();
    }
}

/// Clears the liveness back to `Alive` (unconditional store) — the RESTART path,
/// and the ONLY documented exit from the terminal `Dead` state. Fires `wake` iff
/// the state actually changed.
///
/// NOTE: the RPC-success auto-clear of a transient `NotResponding` is a SEPARATE
/// call-site guard in `block_on_rpc` (a `compare_exchange(NotResponding ->
/// Alive)` that never touches `Dead`), NOT this unconditional store.
pub fn reset_alive(state: &AtomicU8, wake: &WakeFn) {
    let previous = state.swap(Liveness::Alive.to_u8(), Ordering::SeqCst);
    if previous != Liveness::Alive.to_u8() {
        wake();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::{get, mark_dead, mark_not_responding, reset_alive, Liveness};
    use crate::WakeFn;

    /// The FFI discriminant for `Alive` (mirrored by `wolf-ffi`'s `liveness` module).
    const ALIVE_U8: u8 = 0;
    /// The FFI discriminant for `NotResponding`.
    const NOT_RESPONDING_U8: u8 = 1;
    /// The FFI discriminant for `Dead`.
    const DEAD_U8: u8 = 2;

    /// A `WakeFn` that counts invocations, paired with the shared counter so a
    /// test can assert exactly how many wakes a transition fired.
    fn counting_wake() -> (WakeFn, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        let sink = Arc::clone(&count);
        let wake: WakeFn = Arc::new(move || {
            sink.fetch_add(1, Ordering::SeqCst);
        });
        (wake, count)
    }

    /// The number of wakes fired so far.
    fn wakes(count: &Arc<AtomicUsize>) -> usize {
        count.load(Ordering::SeqCst)
    }

    /// An atomic seeded to a given `Liveness` (via its FFI discriminant).
    fn state_at(liveness: Liveness) -> AtomicU8 {
        AtomicU8::new(liveness.to_u8())
    }

    // ---- Discriminants + round-trip -------------------------------------

    #[test]
    fn discriminants_are_stable_zero_one_two() {
        assert_eq!(Liveness::Alive.to_u8(), ALIVE_U8);
        assert_eq!(Liveness::NotResponding.to_u8(), NOT_RESPONDING_U8);
        assert_eq!(Liveness::Dead.to_u8(), DEAD_U8);
    }

    #[test]
    fn from_u8_inverts_to_u8() {
        for liveness in [Liveness::Alive, Liveness::NotResponding, Liveness::Dead] {
            assert_eq!(
                Liveness::from_u8(liveness.to_u8()),
                liveness,
                "from_u8(to_u8({liveness:?})) must round-trip"
            );
        }
    }

    #[test]
    fn get_reads_the_seeded_state() {
        assert_eq!(get(&state_at(Liveness::Alive)), Liveness::Alive);
        assert_eq!(
            get(&state_at(Liveness::NotResponding)),
            Liveness::NotResponding
        );
        assert_eq!(get(&state_at(Liveness::Dead)), Liveness::Dead);
    }

    // ---- mark_not_responding --------------------------------------------

    #[test]
    fn mark_not_responding_transitions_alive_and_fires_one_wake() {
        let state = state_at(Liveness::Alive);
        let (wake, count) = counting_wake();

        mark_not_responding(&state, &wake);

        assert_eq!(get(&state), Liveness::NotResponding);
        assert_eq!(
            wakes(&count),
            1,
            "Alive -> NotResponding must fire one wake"
        );
    }

    #[test]
    fn mark_not_responding_is_idempotent_no_second_wake() {
        let state = state_at(Liveness::Alive);
        let (wake, count) = counting_wake();

        mark_not_responding(&state, &wake);
        mark_not_responding(&state, &wake);

        assert_eq!(get(&state), Liveness::NotResponding);
        assert_eq!(
            wakes(&count),
            1,
            "a second mark_not_responding must NOT fire another wake (no state change)"
        );
    }

    #[test]
    fn mark_not_responding_never_downgrades_dead() {
        let state = state_at(Liveness::Dead);
        let (wake, count) = counting_wake();

        mark_not_responding(&state, &wake);

        assert_eq!(
            get(&state),
            Liveness::Dead,
            "Dead must NOT downgrade to NotResponding"
        );
        assert_eq!(wakes(&count), 0, "a no-op transition must NOT fire a wake");
    }

    // ---- mark_dead ------------------------------------------------------

    #[test]
    fn mark_dead_transitions_and_fires_one_wake() {
        let state = state_at(Liveness::Alive);
        let (wake, count) = counting_wake();

        mark_dead(&state, &wake);

        assert_eq!(get(&state), Liveness::Dead);
        assert_eq!(wakes(&count), 1, "Alive -> Dead must fire one wake");
    }

    #[test]
    fn mark_dead_from_not_responding_fires_one_wake() {
        let state = state_at(Liveness::NotResponding);
        let (wake, count) = counting_wake();

        mark_dead(&state, &wake);

        assert_eq!(get(&state), Liveness::Dead);
        assert_eq!(wakes(&count), 1, "NotResponding -> Dead must fire one wake");
    }

    #[test]
    fn mark_dead_is_terminal_and_idempotent() {
        let state = state_at(Liveness::Alive);
        let (wake, count) = counting_wake();

        mark_dead(&state, &wake);
        mark_dead(&state, &wake);

        assert_eq!(get(&state), Liveness::Dead);
        assert_eq!(
            wakes(&count),
            1,
            "a second mark_dead must NOT fire another wake (already Dead)"
        );
    }

    #[test]
    fn dead_stays_dead_when_a_later_stall_is_reported() {
        // A stall signal (mark_not_responding) arriving AFTER death must not
        // resurrect the (terminal) Dead state — Dead outranks NotResponding.
        let state = state_at(Liveness::Alive);
        let (wake, count) = counting_wake();

        mark_dead(&state, &wake);
        mark_not_responding(&state, &wake);

        assert_eq!(get(&state), Liveness::Dead);
        assert_eq!(
            wakes(&count),
            1,
            "only the mark_dead transition fired; the late stall was a no-op"
        );
    }

    // ---- reset_alive (restart path) -------------------------------------

    #[test]
    fn reset_alive_clears_not_responding_and_fires_one_wake() {
        let state = state_at(Liveness::NotResponding);
        let (wake, count) = counting_wake();

        reset_alive(&state, &wake);

        assert_eq!(get(&state), Liveness::Alive);
        assert_eq!(
            wakes(&count),
            1,
            "NotResponding -> Alive (auto-clear / restart) must fire one wake"
        );
    }

    #[test]
    fn reset_alive_is_noop_when_already_alive() {
        let state = state_at(Liveness::Alive);
        let (wake, count) = counting_wake();

        reset_alive(&state, &wake);

        assert_eq!(get(&state), Liveness::Alive);
        assert_eq!(
            wakes(&count),
            0,
            "reset_alive on an already-Alive state must NOT fire a wake"
        );
    }

    #[test]
    fn reset_alive_leaves_dead_the_restart_exit() {
        // reset_alive is the ONLY documented exit from the terminal Dead state,
        // used by `restart_nvim`. It fires a wake on the change.
        let state = state_at(Liveness::Dead);
        let (wake, count) = counting_wake();

        reset_alive(&state, &wake);

        assert_eq!(
            get(&state),
            Liveness::Alive,
            "reset_alive must leave Dead -> Alive for the restart path"
        );
        assert_eq!(wakes(&count), 1, "Dead -> Alive must fire one wake");
    }
}
