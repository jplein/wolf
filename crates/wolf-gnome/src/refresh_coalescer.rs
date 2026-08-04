//! The pure, GTK-free state machine behind the sidebar's TRAILING refresh.
//!
//! A port of `WolfKeyMap/PickerRefreshCoalescer.swift`, with one deliberate
//! divergence: macOS shares its coalescer between the picker and the file tree,
//! while on Linux only the sidebar adopts it. The picker's refresh is not part of
//! this rule and widening it would put the picker's cage smokes at risk for nothing.
//!
//! A streaming traversal fires a wake per batch. Left alone each wake rebuilds the
//! whole root store, which on `wolf-gnome` also collapses every manually expanded
//! directory. This collapses a burst into one trailing fire per
//! [`REFRESH_INTERVAL`]: [`RefreshCoalescer::request`] answers `Some(generation)`
//! when the caller must arm a one-shot `glib` timeout and `None` when the batch
//! coalesced into an already-armed one.
//!
//! ## The final batch is never dropped
//!
//! Each batch appends to the shared search state and THEN wakes, and a fire reads
//! that state at fire time. A wake therefore either coalesces into an armed fire —
//! which by definition has not run yet, so it will read the state including that
//! batch — or finds nothing armed and arms a new one. [`RefreshCoalescer::fire`]
//! clears `armed` BEFORE it returns, which is what closes the third case: a batch
//! arriving during or after a fire arms a fresh one rather than coalescing into a
//! fire that has already performed. That property is what a frame-budget check
//! (drop the wake if too little time has passed) cannot give, and it is the whole
//! reason the mechanism is a trailing timer rather than a budget.
//!
//! ## Cancellation
//!
//! [`RefreshCoalescer::cancel`] bumps the generation rather than removing the GLib
//! source, because removing an already-fired source is a GLib warning. The armed
//! closure still runs; its `fire` call carries a generation the coalescer has moved
//! past, so it performs nothing. This is the generation-token pattern
//! `crate::crash_overlay`'s banner already uses.
//!
//! [`REFRESH_INTERVAL`]'s value is asserted nowhere: a constant compared against its
//! own literal states nothing. What the tests pin is the mechanism.

use std::time::Duration;

/// How long a trailing refresh waits before it fires — roughly 30 fps.
///
/// **The one site in this crate where this number lives.** The macOS counterpart is
/// `PickerRefreshCoalescer.refreshInterval = 0.033`.
pub const REFRESH_INTERVAL: Duration = Duration::from_millis(33);

/// The trailing-refresh state machine: at most one armed fire at a time, plus a
/// generation token that makes a cancelled fire a no-op.
pub struct RefreshCoalescer {
    /// Whether a fire is armed and has not yet performed.
    armed: bool,
    /// The token the currently armed fire must carry to perform.
    generation: u64,
}

impl RefreshCoalescer {
    /// A coalescer with nothing armed.
    #[must_use]
    pub fn new() -> RefreshCoalescer {
        RefreshCoalescer {
            armed: false,
            generation: 0,
        }
    }

    /// Records one refresh request, answering `Some(generation)` when the caller
    /// must arm a trailing fire carrying that generation, and `None` when the
    /// request coalesced into one that is already armed.
    #[must_use]
    pub fn request(&mut self) -> Option<u64> {
        if self.armed {
            return None;
        }
        self.armed = true;
        Some(self.generation)
    }

    /// Whether the fire carrying `generation` should perform its refresh.
    ///
    /// Disarms FIRST when it does, so a batch arriving during the refresh arms a
    /// new fire instead of coalescing into this one — see the module docs. A stale
    /// generation (from a cancelled source) leaves any newly armed fire alone.
    pub fn fire(&mut self, generation: u64) -> bool {
        let performs = self.armed && generation == self.generation;
        if performs {
            self.armed = false;
        }
        performs
    }

    /// Disarms, and moves to a generation the already-queued GLib closure cannot
    /// match. Called by the sidebar's re-root and close.
    pub fn cancel(&mut self) {
        self.armed = false;
        self.generation = self.generation.wrapping_add(1);
    }
}

impl Default for RefreshCoalescer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every test here drives "the trailing timer fired" BY HAND, by calling
    // `fire` with the generation `request` handed out. Nothing sleeps, nothing
    // reads a clock, and `REFRESH_INTERVAL`'s value is never asserted on: a
    // constant compared against its own literal states nothing. What is asserted
    // is the mechanism — how many fires a burst arms, and that a batch arriving
    // after a fire arms another one.

    /// A burst of traversal-batch wakes arms exactly ONE trailing fire.
    ///
    /// The four `request()` calls stand for four streamed batches inside one
    /// refresh window. Only the first may ask the caller to arm a `glib` timeout;
    /// the other three must coalesce into it, which is the whole point of the
    /// seam — without it each batch re-runs the O(N) rebuild.
    ///
    /// Revert that must redden it: making `request()` always return `Some(..)`
    /// fails requests 2, 3 and 4 (four armed fires instead of one).
    #[test]
    fn a_burst_arms_exactly_one_fire() {
        let mut coalescer = RefreshCoalescer::new();

        let first = coalescer.request();
        let second = coalescer.request();
        let third = coalescer.request();
        let fourth = coalescer.request();

        assert!(
            first.is_some(),
            "request 1 of 4 must arm a trailing fire: with nothing armed, a batch that returned \
             None would never be drawn at all"
        );
        assert_eq!(
            second, None,
            "request 2 of 4 must coalesce into the fire request 1 armed, but asked to arm \
             another ({second:?}). Each extra armed fire is another full rebuild of the root \
             store during the traversal."
        );
        assert_eq!(
            third, None,
            "request 3 of 4 must coalesce into the armed fire, but asked to arm another \
             ({third:?})"
        );
        assert_eq!(
            fourth, None,
            "request 4 of 4 must coalesce into the armed fire, but asked to arm another \
             ({fourth:?})"
        );
    }

    /// A batch arriving AFTER a fire arms a new fire, so the last batch of a
    /// traversal is never left undrawn.
    ///
    /// This is what row 3 costs on Linux: a wake routed to one view while another
    /// view's traversal runs — the macOS defect — has no counterpart in a
    /// single-window front-end, so the guarantee this seam carries is that the
    /// coalescer itself cannot drop a traversal's final batch. A wake either
    /// coalesces into an armed fire — which has not run yet, so it will read the
    /// state that batch appended — or it finds nothing armed and arms a new one.
    /// This test pins the second case, which is the only one where the ordering
    /// inside `fire` decides the answer.
    ///
    /// Revert that must redden it: not clearing `armed` inside `fire` before it
    /// returns, so the request after the fire coalesces into a fire that has
    /// already run and the final batch is dropped.
    #[test]
    fn final_request_after_a_fire_arms_a_new_one() {
        let mut coalescer = RefreshCoalescer::new();

        let armed = coalescer.request();
        let Some(generation) = armed else {
            panic!(
                "the first request against a fresh coalescer must arm a fire, but coalesced into \
                 nothing (returned None)"
            );
        };

        let fired = coalescer.fire(generation);
        assert!(
            fired,
            "the fire armed by the first request must perform the refresh when its timeout runs; \
             a false here means the burst is collapsed into no rebuild at all"
        );

        let after_the_fire = coalescer.request();
        assert!(
            after_the_fire.is_some(),
            "a batch arriving after the fire has run must arm a NEW trailing fire, but returned \
             None — it coalesced into a fire that has already performed its refresh, so the \
             entries that batch appended are never drawn. That is exactly the \"final batch of a \
             traversal is lost\" hazard this seam must not introduce."
        );
    }

    /// A second fire at the same generation performs nothing.
    ///
    /// A `glib::timeout_add_local_once` source cannot fire twice, but a cancelled
    /// one can still run its closure, and both routes end in the same call. The
    /// armed flag — cleared by the first fire — is what makes the second one a
    /// no-op rather than a duplicate rebuild.
    ///
    /// Revert that must redden it: dropping the `armed` term from `fire`'s
    /// condition, leaving only the generation comparison, so the second fire also
    /// returns true.
    #[test]
    fn a_stale_duplicate_fire_performs_nothing() {
        let mut coalescer = RefreshCoalescer::new();

        let Some(generation) = coalescer.request() else {
            panic!(
                "the first request against a fresh coalescer must arm a fire, but coalesced into \
                 nothing (returned None)"
            );
        };

        let first_fire = coalescer.fire(generation);
        assert!(
            first_fire,
            "the first fire at the armed generation must perform the refresh"
        );

        let second_fire = coalescer.fire(generation);
        assert!(
            !second_fire,
            "a second fire at the same generation must perform NOTHING: the burst it was armed \
             for has already been refreshed, so performing again is a duplicate full rebuild that \
             collapses every manually expanded directory"
        );
    }

    /// `cancel()` disarms, and the next request arms afresh at a generation the
    /// stale fire cannot match.
    ///
    /// `reroot()` and `close()` both cancel: a re-root rebuilds wholesale and runs
    /// its own immediate refresh, and a close drops the search. An already-armed
    /// GLib source still runs its closure — removing it would be a GLib warning —
    /// so the generation token is what turns that late closure into a genuine
    /// no-op. Only the INEQUALITY of the two generations is asserted; how the
    /// number advances is an implementation detail.
    ///
    /// Revert that must redden it: making `cancel()` a no-op, which leaves the
    /// stale fire performing (assertion 2) and the next request coalescing into
    /// the cancelled fire instead of arming (assertion 3).
    #[test]
    fn cancel_disarms_and_the_next_request_rearms_at_a_new_generation() {
        let mut coalescer = RefreshCoalescer::new();

        let Some(cancelled_generation) = coalescer.request() else {
            panic!(
                "the first request against a fresh coalescer must arm a fire, but coalesced into \
                 nothing (returned None)"
            );
        };

        coalescer.cancel();

        let stale_fire = coalescer.fire(cancelled_generation);
        assert!(
            !stale_fire,
            "the GLib source armed before the cancel still runs its closure, so the fire it makes \
             must perform NOTHING: after a re-root it would otherwise refresh the sidebar from \
             the entries of the directory the user just left"
        );

        let rearmed = coalescer.request();
        let Some(new_generation) = rearmed else {
            panic!(
                "the first request after a cancel must arm a fire, but returned None — it \
                 coalesced into the cancelled fire, which will never perform, so the sidebar \
                 would stop refreshing entirely after a re-root"
            );
        };
        assert_ne!(
            new_generation, cancelled_generation,
            "the re-armed fire must carry a generation the cancelled one cannot match \
             ({new_generation} == {cancelled_generation}): otherwise the stale GLib closure, \
             which is still queued, would satisfy the new fire and refresh at the wrong moment"
        );
    }
}
