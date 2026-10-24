//! The pure, GTK-free state machine behind the Markdown preview's COALESCED render.
//!
//! Typing beside a live preview issues one BLOCKING `render_preview` RPC per
//! keystroke — the only blocking call this front-end repeats while the user simply
//! types, and `notes/wolf-gnome-decisions.md` ("Frame-time instrumentation") holds
//! what it measured. This collapses a burst into three kinds of render and no
//! others: one on the FIRST edit after a quiet gap, one whenever the burst has run
//! [`PREVIEW_MAX_STALENESS`] without a render, and one after the burst ends.
//!
//! It reads no clock of its own — every entry point takes `now: Instant` from the
//! caller — and it owns no widget, no `WolfCore` and no GLib source. Deciding is
//! all it does; the caller arms the timer, performs the render and reports it back
//! through [`PreviewCoalescer::rendered`].
//!
//! ## Its relationship to [`crate::refresh_coalescer`]
//!
//! The same generation-token mechanism, a DIFFERENT rule, and the two must not be
//! merged. `RefreshCoalescer` is clock-free by construction — "is a fire already
//! armed?" is the whole of its question — and it is a port of a macOS seam whose
//! proven tests would all have to be rewritten around a clock to widen it. The
//! sidebar wants neither a leading edge nor a staleness cap, and this rule is
//! nothing but those two.
//!
//! ## The leading edge needs BOTH of its conjuncts
//!
//! A request renders inline only when nothing is pending AND the last render is at
//! least [`PREVIEW_LEADING_IDLE`] old. Neither half survives alone:
//!
//! - Without "nothing is pending", every keystroke of a long burst whose last
//!   render has aged past the idle threshold renders inline, which is the
//!   per-keystroke RPC this seam exists to remove.
//! - Without the idle threshold, a trailing fire clears the pending burst and the
//!   very next keystroke finds nothing pending and renders inline — back to back
//!   with the fire that just ran. In a steady stream of typing every keystroke
//!   becomes its own leading edge and the coalescing is gone.
//!
//! ## The deadline is the MINIMUM of two terms
//!
//! One term is [`PREVIEW_TRAILING_DELAY`], measured from this request, so the fire
//! tracks the LAST keystroke of a burst rather than the first. The other is what
//! remains of [`PREVIEW_MAX_STALENESS`] since the burst began, so a user who never
//! pauses still sees the document refresh instead of never, on the period that
//! constant's own docs derive. Whichever deadline is nearer wins; once the staleness
//! budget is spent the answer is `Duration::ZERO`, which arms a fire for the next
//! main-loop turn rather than skipping one.
//!
//! ## One fresh one-shot per request
//!
//! Every `Arm` answer carries a NEW generation, so the sources armed by a burst's
//! earlier keystrokes still run their closures and perform nothing. Two shapes were
//! rejected. A `glib::timeout_add_local` returning `ControlFlow::Continue` is a
//! genuinely repeating source — exactly the periodic timer `notes/wolf-gnome-decisions.md` forbids.
//! One live source that re-arms itself for the remaining time makes `fire`
//! three-way and moves arming into the fire closure, to save a `GSource` attach
//! that costs microseconds against a multi-millisecond RPC. What the chosen shape
//! buys is the property `notes/wolf-gnome-decisions.md` states: live sources are bounded by
//! `ceil(PREVIEW_TRAILING_DELAY / inter-key interval)` during a fast burst — a
//! reader who finds five queued at once has found the design, not a leak — and
//! there are ZERO of them while the user is idle.
//!
//! ## There is no `cancel`
//!
//! [`crate::refresh_coalescer::RefreshCoalescer::cancel`] exists because a re-root
//! makes the armed fire's CONTENT wrong. Here the fire re-derives the pane set, the
//! liveness reading and the document from live state at fire time, so there is no
//! wrong content to cancel: a pane that closed mid-burst is simply no longer due.
//! Every teardown path is already covered twice over, by the closure's
//! `Weak<PaneInner>` upgrade and by `preview_ui::may_render` answering false once
//! the core has been taken out.
//!
//! The three constants are tuned to a 150 ms trailing delay, a 500 ms staleness cap
//! and a 150 ms leading idle. No test compares any of them against its own literal:
//! what the tests pin is the rule.

use std::time::{Duration, Instant};

/// How long after the newest edit a trailing render waits.
///
/// **The one site in this crate where this number lives.** Above a fast typist's
/// inter-key interval, so a burst really does collapse, and below the ~200 ms at
/// which a delayed update starts to read as lag.
pub const PREVIEW_TRAILING_DELAY: Duration = Duration::from_millis(150);

/// The longest a burst may go unrendered.
///
/// A pure trailing debounce renders NEVER while the user keeps typing. This bounds
/// that wait — but it is a PERIOD OF TWO TERMS, never a rate. A render clears the
/// pending burst and the clock restarts at the next REQUEST rather than at the
/// render, so sustained typing renders once per this value plus the inter-key
/// interval: about 535 ms at 35 ms a key, which is under two a second and never
/// exactly two. Measured under `cage`, one uninterrupted 10.343 s burst paid 20
/// renders and one of 2.704 s paid 6, each a leading edge, the cap renders between
/// them and a trailing fire.
pub const PREVIEW_MAX_STALENESS: Duration = Duration::from_millis(500);

/// The quiet, measured since the last render, that re-arms the leading edge.
///
/// **Must never be shorter than [`PREVIEW_TRAILING_DELAY`]**, or a keystroke
/// arriving just after a trailing fire takes the leading edge and renders back to
/// back with it. It is a separate constant because it answers a different question:
/// folding the two would make a retune of the trailing delay silently move the
/// leading-edge threshold as well.
pub const PREVIEW_LEADING_IDLE: Duration = Duration::from_millis(150);

/// What a caller must do about one request to render the preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderDecision {
    /// Render inline, on this wake. The caller's render pass must end by calling
    /// [`PreviewCoalescer::rendered`]; answering `Render` does not itself record
    /// one.
    Render,
    /// Arm a one-shot `glib` timeout for `delay`, whose closure calls
    /// [`PreviewCoalescer::fire`] with `generation` and renders only if it answers
    /// true.
    Arm {
        /// The token the armed fire must carry to perform.
        generation: u64,
        /// How long that one-shot waits.
        delay: Duration,
    },
}

/// The preview-render state machine: at most one LIVE generation, a burst's start
/// instant, and the instant a render last performed.
pub struct PreviewCoalescer {
    /// When the current unrendered burst began, or `None` when nothing is pending.
    pending_since: Option<Instant>,
    /// When a render pass last actually performed.
    last_render: Option<Instant>,
    /// Whether a fire is armed and has not yet performed.
    armed: bool,
    /// The token the currently armed fire must carry to perform.
    generation: u64,
}

impl PreviewCoalescer {
    /// A coalescer with nothing pending and nothing armed.
    #[must_use]
    pub fn new() -> PreviewCoalescer {
        PreviewCoalescer {
            pending_since: None,
            last_render: None,
            armed: false,
            generation: 0,
        }
    }

    /// Records that at least one preview is due at `now`, answering whether the
    /// caller renders inline or arms a trailing fire.
    ///
    /// Called ONLY when a pane is genuinely due: a request on every wake would keep
    /// a timer armed continuously while the user types with no preview open.
    #[must_use]
    pub fn request(&mut self, now: Instant) -> RenderDecision {
        let nothing_was_pending = self.pending_since.is_none();
        let pending_since = *self.pending_since.get_or_insert(now);
        // The leading edge, and BOTH of its conjuncts — the module docs name the
        // failure each one alone would let through.
        if nothing_was_pending
            && self
                .last_render
                .is_none_or(|last| now.saturating_duration_since(last) >= PREVIEW_LEADING_IDLE)
        {
            return RenderDecision::Render;
        }

        // Durations only: `Instant + Duration` panics on overflow. The nearer budget wins, and a spent
        // staleness budget saturates to zero rather than wrapping.
        let elapsed = now.saturating_duration_since(pending_since);
        let delay = PREVIEW_TRAILING_DELAY.min(PREVIEW_MAX_STALENESS.saturating_sub(elapsed));
        self.armed = true;
        // Bumped on EVERY arm, which is what supersedes the source armed by the
        // previous keystroke and makes the trailing edge track the burst's last one.
        self.generation = self.generation.wrapping_add(1);
        RenderDecision::Arm {
            generation: self.generation,
            delay,
        }
    }

    /// Whether the fire carrying `generation` should perform its render pass.
    ///
    /// Disarms FIRST when it does, so an edit arriving during the pass arms a new
    /// fire instead of coalescing into one that has already performed — the
    /// [`crate::refresh_coalescer::RefreshCoalescer::fire`] rule. A superseded
    /// generation leaves the live one alone.
    pub fn fire(&mut self, generation: u64) -> bool {
        let performs = self.armed && generation == self.generation;
        if performs {
            self.armed = false;
        }
        performs
    }

    /// Records that a render pass performed at `now`: the burst is no longer
    /// pending, the leading edge starts ageing again, and nothing is armed.
    ///
    /// Called from exactly ONE funnel — the last statement of the single method
    /// that performs a render pass — so no caller can forget it.
    pub fn rendered(&mut self, now: Instant) {
        self.pending_since = None;
        self.last_render = Some(now);
        self.armed = false;
    }
}

impl Default for PreviewCoalescer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The timer is driven by hand and every `Instant` is one `now()` plus a `Duration`; nothing sleeps.
    // See notes/crates/wolf-gnome/src/preview_coalescer.rs.md#test-clock-and-constants.

    /// The `Arm` payload, or a panic naming the request that rendered instead.
    fn armed(decision: RenderDecision, what: &str) -> (u64, Duration) {
        match decision {
            RenderDecision::Arm { generation, delay } => (generation, delay),
            RenderDecision::Render => panic!(
                "{what} must ARM a trailing fire, but the coalescer answered Render — it took the \
                 leading edge in the middle of a burst. That is one blocking render_preview per \
                 keystroke, which is exactly the cost this seam exists to remove."
            ),
        }
    }

    /// The first edit after an idle gap renders IMMEDIATELY, and a later gap re-arms
    /// that leading edge.
    ///
    /// This is the half of the budget the user feels: opening a preview, or
    /// touching a document after a pause, must put the change on screen at once
    /// rather than after a debounce nobody asked for.
    ///
    /// Revert that must redden it: delete the early-`Render` branch so `request`
    /// always answers `Arm`. Cost: the first keystroke after any pause waits a whole
    /// trailing delay for a preview that could have been current immediately.
    #[test]
    fn the_first_edit_after_an_idle_gap_renders_immediately() {
        let t0 = Instant::now();
        let mut coalescer = PreviewCoalescer::new();

        let cold = coalescer.request(t0);
        assert_eq!(
            cold,
            RenderDecision::Render,
            "the FIRST request against a fresh coalescer must render inline, but it answered \
             {cold:?}. A fresh coalescer is the state a preview pane is opened in, so an `Arm` \
             here means the pane sits empty for a trailing delay before its document ever appears."
        );

        // What the caller's render pass does as its last statement.
        coalescer.rendered(t0);

        let after_a_long_idle = coalescer.request(t0 + Duration::from_secs(10));
        assert_eq!(
            after_a_long_idle,
            RenderDecision::Render,
            "ten seconds after the last render is far longer than any burst, so this request must \
             take the leading edge again, but it answered {after_a_long_idle:?}. An `Arm` here \
             means the leading edge is spent once and never re-arms: every edit for the rest of \
             the session is deferred, and the reader waits a trailing delay for each one."
        );
    }

    /// A burst of edits arms one fire per keystroke, and ONLY the newest one
    /// performs.
    ///
    /// The three requests stand for three keystrokes inside one trailing window.
    /// Each supersedes the fire before it, which is what makes the trailing edge
    /// track the burst's LAST keystroke; the superseded GLib closures still run —
    /// removing an armed source is a GLib warning — and must perform nothing.
    ///
    /// Revert that must redden it: stop bumping `generation` in `request`. All three
    /// sources then satisfy their fire, so one burst issues three blocking renders,
    /// and the trailing edge lands a full delay after the burst's FIRST keystroke
    /// rather than its last.
    #[test]
    fn a_burst_arms_one_fire_and_only_the_newest_generation_performs() {
        let t0 = Instant::now();
        let mut coalescer = PreviewCoalescer::new();

        // A render has just performed, so the leading edge is spent and every
        // request below is a trailing one — the state a burst is typed into.
        coalescer.rendered(t0);

        let (first, _) = armed(
            coalescer.request(t0 + Duration::from_millis(40)),
            "keystroke 1 of 3",
        );
        let (second, _) = armed(
            coalescer.request(t0 + Duration::from_millis(80)),
            "keystroke 2 of 3",
        );
        let (third, _) = armed(
            coalescer.request(t0 + Duration::from_millis(120)),
            "keystroke 3 of 3",
        );

        assert_ne!(
            first, second,
            "keystrokes 1 and 2 armed fires carrying the SAME generation ({first}), so the second \
             does not supersede the first. Both would perform, and the burst pays two blocking \
             renders where it should pay one."
        );
        assert_ne!(
            second, third,
            "keystrokes 2 and 3 armed fires carrying the same generation ({second}); see the \
             message above."
        );
        assert_ne!(
            first, third,
            "keystrokes 1 and 3 armed fires carrying the same generation ({first}). A generation \
             that repeats within one burst is the same defect as one that never moves, only \
             harder to see."
        );

        assert!(
            !coalescer.fire(first),
            "the source armed by keystroke 1 still runs its closure — an armed GLib one-shot \
             cannot be withdrawn without a warning — so it must perform NOTHING. Performing means \
             a blocking render of a document that is already two keystrokes out of date, on the \
             main thread, while the user is still typing."
        );
        assert!(
            !coalescer.fire(second),
            "the source armed by keystroke 2 is superseded in the same way and must perform \
             nothing either. Two of the three firing is the same user-visible cost as all three: \
             the typing stutters."
        );
        assert!(
            coalescer.fire(third),
            "the fire armed by the burst's LAST keystroke must perform: it is the trailing edge, \
             and it is the only thing that puts the end of the burst on screen. A false here \
             means the preview freezes at whatever it last showed until the user types again — \
             the worst failure this seam can have, because it looks like a broken preview rather \
             than a slow one."
        );
    }

    /// The armed delay never exceeds the trailing delay, and inside a burst it is
    /// capped by what remains of the staleness budget — reaching zero at the
    /// deadline and staying there.
    ///
    /// This is the only evidence anywhere that the 500 ms cap exists: a user who
    /// types without pausing gets no trailing edge at all, so without the cap term
    /// the preview would not refresh until they stopped.
    ///
    /// Revert that must redden it: drop the `min` against the cap's remainder, so
    /// the delay is always the trailing delay. A continuously typing user then sees
    /// nothing until they pause, and a burst that has already run past the cap waits
    /// another full trailing delay instead of re-arming for the next main-loop turn.
    #[test]
    fn the_delay_is_capped_by_the_staleness_deadline() {
        // Milliseconds from `t0`. The burst begins at `BURST_START_MS`, which is `pending_since` for the
        // whole test and the origin of every elapsed figure.
        const BURST_START_MS: u64 = 40;
        const BURST_STEP_MS: u64 = 40;
        const BURST_STEPS: u64 = 9;
        const CAP_PROBE_ELAPSED_MS: u64 = 400;

        // Premise: at the probe the cap's remainder is below the trailing delay, or `min` picks the
        // trailing term and the probe says nothing about the cap.
        let cap_remainder =
            PREVIEW_MAX_STALENESS.saturating_sub(Duration::from_millis(CAP_PROBE_ELAPSED_MS));
        let trailing = PREVIEW_TRAILING_DELAY;
        assert!(
            cap_remainder < trailing,
            "premise unmet: {CAP_PROBE_ELAPSED_MS} ms into a burst the staleness budget has \
             {cap_remainder:?} left, which is not shorter than the trailing delay {trailing:?}, so \
             the deadline's `min` answers the trailing term at this probe and the assertion below \
             would hold with the cap deleted. Move the probe deeper into the burst; do not weaken \
             the assertion."
        );

        let t0 = Instant::now();
        let mut coalescer = PreviewCoalescer::new();
        coalescer.rendered(t0);

        // A user typing steadily, with nothing rendered in between.
        for step in 0..BURST_STEPS {
            let at_ms = BURST_START_MS + step * BURST_STEP_MS;
            let (_, delay) = armed(
                coalescer.request(t0 + Duration::from_millis(at_ms)),
                &format!("the request {at_ms} ms into the session"),
            );
            assert!(
                delay <= trailing,
                "the request {at_ms} ms in armed a fire {delay:?} out, which is longer than the \
                 trailing delay {trailing:?}. Nothing may ever wait longer than that after the \
                 newest edit: a delay above it is a preview that lags the keyboard by more than \
                 the budget the whole design is built around."
            );
        }

        let capped_at = t0 + Duration::from_millis(BURST_START_MS + CAP_PROBE_ELAPSED_MS);
        let (_, capped) = armed(coalescer.request(capped_at), "the request at the cap probe");
        assert_eq!(
            capped, cap_remainder,
            "{CAP_PROBE_ELAPSED_MS} ms into an unrendered burst the armed delay must be exactly \
             what is left of the staleness budget ({cap_remainder:?}), but it was {capped:?}. The \
             trailing delay {trailing:?} here means the `min` against the cap is gone, and a user \
             who keeps typing never reaches a pause, so the preview would stop updating for as \
             long as the burst lasts."
        );

        let deadline_at = t0 + Duration::from_millis(BURST_START_MS) + PREVIEW_MAX_STALENESS;
        let (_, at_deadline) = armed(
            coalescer.request(deadline_at),
            "the request at the staleness deadline",
        );
        assert_eq!(
            at_deadline,
            Duration::ZERO,
            "at the staleness deadline itself the budget is spent, so the fire must be armed for \
             the next main-loop turn (a zero delay), but it was armed {at_deadline:?} out. This is \
             also the mechanism a fire the liveness gate refused recovers through: the burst stays \
             pending and the next due wake re-arms immediately, which a non-zero delay here turns \
             into another full wait."
        );

        let past_at = deadline_at + Duration::from_millis(100);
        let (_, past_deadline) = armed(
            coalescer.request(past_at),
            "the request past the staleness deadline",
        );
        assert_eq!(
            past_deadline,
            Duration::ZERO,
            "past the staleness deadline the delay must stay at zero, but it was \
             {past_deadline:?}. A non-zero answer here means the remaining budget was computed by \
             a subtraction that wrapped or went negative rather than saturating at zero."
        );
    }

    /// An edit arriving just after a render does NOT take a second leading edge.
    ///
    /// The keystroke that follows a trailing fire finds nothing pending, so the
    /// idle threshold is the only thing between it and an inline render. Without it
    /// two blocking renders land back to back, and in a steady stream of typing
    /// every keystroke becomes its own leading edge — the seam then costs exactly
    /// what it cost before it existed.
    ///
    /// Revert that must redden it: drop the [`PREVIEW_LEADING_IDLE`] conjunct from
    /// `request`'s leading-edge condition. It is the single most likely edit in this
    /// file, because the remaining condition still reads plausibly.
    ///
    /// **Measured: that revert reddens FOUR of this module's five tests, and this is
    /// the one that names the reason.** Tests 2, 3 and 5 each open their burst with
    /// `rendered(t0)` and then `request(t0 + 40 ms)`, and every one of them assumes
    /// that second request ARMS; without the conjunct it renders instead, so all
    /// three die inside the `armed()` helper before they reach their own subject. That
    /// is the measure of how much this one conjunct carries — three unrelated
    /// properties are statable only because it holds — rather than a fault in the
    /// falsifier. No smaller edit isolates this test: the conjunct is exactly what
    /// the other three depend on too.
    #[test]
    fn an_edit_just_after_a_render_does_not_take_a_second_leading_edge() {
        // Premise, and the invariant the two constants carry: the leading-edge quiet is never shorter
        // than the trailing delay, or a key one trailing delay after a fire is a real second leading edge.
        let leading = PREVIEW_LEADING_IDLE;
        let trailing = PREVIEW_TRAILING_DELAY;
        assert!(
            leading >= trailing,
            "premise unmet: PREVIEW_LEADING_IDLE is {leading:?}, shorter than \
             PREVIEW_TRAILING_DELAY {trailing:?}. At that tuning a keystroke arriving just after a \
             trailing fire re-takes the leading edge and renders back to back with it — two \
             blocking RPCs a trailing delay apart, which is the pattern the budget forbids. Retune \
             both together or not at all."
        );

        let t0 = Instant::now();
        let mut coalescer = PreviewCoalescer::new();

        // The leading edge, spent: the burst's first keystroke rendered inline.
        coalescer.rendered(t0);

        let (generation, _) = armed(
            coalescer.request(t0 + Duration::from_millis(40)),
            "the burst's next keystroke",
        );
        assert!(
            coalescer.fire(generation),
            "the fire armed inside the burst must perform when its timeout runs; without that \
             this test never reaches the case it exists for"
        );
        // The trailing pass records its render, which clears the pending burst — the
        // state that makes the next keystroke look like a leading edge.
        coalescer.rendered(t0 + Duration::from_millis(190));

        let next = coalescer.request(t0 + Duration::from_millis(200));
        assert!(
            matches!(next, RenderDecision::Arm { .. }),
            "the keystroke 10 ms after a trailing render answered {next:?}, so it took a SECOND \
             leading edge: two blocking render_preview calls back to back on the main thread. The \
             idle conjunct is what refuses this, and with it gone every keystroke of a steady \
             stream renders inline — each one clears the pending burst, so the next one finds \
             nothing pending and renders too. The seam then costs exactly what it cost before it \
             existed. Expect three companions in the failure list: tests 2, 3 and 5 each assume a \
             burst's second request ARMS, so under this revert they die inside the `armed()` \
             helper before reaching their own subject. This is the test that names the cause — \
             measured, the conjunct is what all four rest on, and no smaller edit isolates any of \
             them."
        );
    }

    /// A second fire at the same generation performs nothing.
    ///
    /// A `glib::timeout_add_local_once` source cannot fire twice, but nothing about
    /// the type system stops a caller reaching `fire` twice for one arm, and the
    /// `armed` flag — cleared by the first fire — is what makes the second a no-op
    /// rather than a duplicate blocking render.
    ///
    /// Revert that must redden it: drop the `armed` term from `fire`'s condition,
    /// leaving only the generation comparison.
    #[test]
    fn a_duplicate_fire_at_the_same_generation_performs_nothing() {
        let t0 = Instant::now();
        let mut coalescer = PreviewCoalescer::new();

        coalescer.rendered(t0);
        let (generation, _) = armed(
            coalescer.request(t0 + Duration::from_millis(40)),
            "the burst's only keystroke",
        );

        assert!(
            coalescer.fire(generation),
            "the first fire at the armed generation must perform the render pass"
        );
        assert!(
            !coalescer.fire(generation),
            "a SECOND fire at the same generation must perform nothing: the burst it was armed \
             for has already been rendered, so performing again is a duplicate blocking RPC for a \
             document that has not changed since"
        );
    }
}
