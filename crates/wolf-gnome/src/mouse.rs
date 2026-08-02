//! Pure, widget-free mouse/scroll translation seams for `wolf-gnome`.
//!
//! Mirrors [`crate::input`] (the key seam): GDK value types in, `wolf_core` input
//! values out, with no widget, no display and no event loop — so every rule here
//! is unit-tested headlessly in this module's own `#[cfg(test)] mod tests`.
//!
//! Ported BEHAVIOURALLY (not literally) from
//! `crates/wolf-macos/Sources/WolfKeyMap/ScrollAccumulator.swift` and
//! `PaneContainerView.swift`'s `mouseEvent` / `scrollWheel`, with the divergences
//! noted per item. Three of those divergences are deliberate and load-bearing:
//!
//! 1. [`scroll_steps`] maps a POSITIVE step count to
//!    [`wolf_core::ScrollDirection::Down`] — the OPPOSITE of the macOS mapping.
//!    Read that function's doc comment before "fixing" it against the Swift.
//! 2. [`ScrollAccumulator::consume`] is arithmetic (truncate + clamp), not the two
//!    unbounded `while` loops the Swift original uses, and it guards non-finite
//!    inputs.
//! 3. [`PaneScroll`] pulls the reset-the-carry-when-the-target-grid-changes rule
//!    DOWN into this module. macOS keeps it in the view
//!    (`PaneContainerView.scrollWheel`'s `if grid != lastScrollGrid { reset() }`),
//!    which on this side of the port would put it in `pane_view.rs`, where it
//!    needs a live display and nothing can reach it.
//!
//! What this module deliberately does NOT do: it never reads a snapshot and never
//! converts pixels to cells. The grid comes from the live snapshot and the cell
//! comes from [`crate::cell_at`]. [`PaneScroll`] is handed a grid id, but only as
//! an OPAQUE identity token it compares for equality — it never resolves one — so
//! every rule in this file stays decidable from its arguments alone.

/// One `GDK_SCROLL_UNIT_WHEEL` detent is one line step, regardless of cell height:
/// a wheel delta is already expressed in detents, not in pixels.
const WHEEL_THRESHOLD: f64 = 1.0;

/// Hard cap on the number of steps a single scroll event may produce. A delta
/// bigger than this is garbage (or hostile), and must not be able to enqueue an
/// unbounded burst of `wolf_core::ScrollEvent`s.
const MAX_STEPS_PER_EVENT: i32 = 256;

/// `MAX_STEPS_PER_EVENT` as an `f64`, for the clamp that happens BEFORE the cast
/// (which is what makes the cast exact and in range by construction).
///
/// DERIVED from [`MAX_STEPS_PER_EVENT`] rather than restated, so the two spellings
/// of the cap cannot drift apart. `f64::from` is not `const`, hence the `as` — and
/// `i32 -> f64` is exact for every `i32`, so the cast loses nothing.
const MAX_STEPS_PER_EVENT_F: f64 = MAX_STEPS_PER_EVENT as f64;

/// The button + modifier state of a GDK pointer event, reduced to what
/// `wolf_core` can actually carry.
///
/// Deliberately NOT a `wolf_core::MouseEvent`: the target grid comes from the live
/// snapshot and the grid-local `row`/`col` come from a different seam
/// ([`crate::cell_at`]), so this type carries only what GDK itself reports about
/// the button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseInput {
    /// Which button. Always `wolf_core::MouseButton::Left` today — see
    /// [`mouse_input`] for why every other button is refused outright.
    pub button: wolf_core::MouseButton,
    /// Control modifier (`gdk::ModifierType::CONTROL_MASK`).
    pub ctrl: bool,
    /// Shift modifier (`gdk::ModifierType::SHIFT_MASK`).
    pub shift: bool,
    /// Alt modifier (`gdk::ModifierType::ALT_MASK`).
    pub alt: bool,
}

/// Translates a GDK button number + modifier state into a [`MouseInput`], or
/// `None` for a button `wolf-gnome` does not forward.
///
/// Only `gdk::BUTTON_PRIMARY` (1) is forwarded; EVERY other button number returns
/// `None`. That is deliberate parity with macOS, which overrides only
/// `mouseDown` / `mouseDragged` / `mouseUp` (the left button): right-click context
/// menus and middle-click paste are separate, unimplemented features on BOTH
/// platforms. `GtkGestureSingle`'s default `button` property is already
/// `GDK_BUTTON_PRIMARY`, so in practice the gesture never fires for another
/// button — the `None` arm is defence-in-depth, plus the one place this decision
/// is recorded and tested.
///
/// Modifiers: `ctrl` from `CONTROL_MASK`, `shift` from `SHIFT_MASK`, `alt` from
/// `ALT_MASK`. `SUPER_MASK` is DELIBERATELY IGNORED: Neovim has no meta mouse
/// modifier (`wolf_core::input::apply_mouse` builds only `"C-"`, `"S-"` and
/// `"A-"`, in that fixed order), so forwarding it would be meaningless. Any other
/// modifier bit (`LOCK_MASK` for Caps Lock, the button masks, …) is outside that
/// grammar and is likewise ignored.
#[must_use]
pub fn mouse_input(button: u32, state: gdk::ModifierType) -> Option<MouseInput> {
    if button != gdk::BUTTON_PRIMARY {
        return None;
    }

    Some(MouseInput {
        button: wolf_core::MouseButton::Left,
        ctrl: state.contains(gdk::ModifierType::CONTROL_MASK),
        shift: state.contains(gdk::ModifierType::SHIFT_MASK),
        alt: state.contains(gdk::ModifierType::ALT_MASK),
        // Every other bit in `state` — SUPER_MASK above all, plus LOCK_MASK and the
        // button masks — is dropped on purpose: nvim's mouse grammar is only
        // "C-"/"S-"/"A-".
    })
}

/// The scroll delta — expressed in whatever unit GDK reported — that constitutes
/// exactly ONE line step.
///
/// | `unit` | threshold |
/// | --- | --- |
/// | `gdk::ScrollUnit::Wheel` | `WHEEL_THRESHOLD` (one detent is one step) |
/// | `gdk::ScrollUnit::Surface` | `cell_h`, when it is finite and `> 0.0`; otherwise `WHEEL_THRESHOLD` |
/// | anything else (`__Unknown`) | `WHEEL_THRESHOLD` |
///
/// `gdk::ScrollUnit` is `#[non_exhaustive]` and carries an `__Unknown(i32)`
/// variant, so a catch-all arm is forced. It falls back to the WHEEL threshold on
/// purpose: guessing WHEEL for a pixel-shaped delta makes scrolling
/// OVER-SENSITIVE — immediately visible, and recoverable by the user — whereas
/// guessing SURFACE for a detent-shaped delta would need roughly `cell_h` detents
/// per line, i.e. scrolling would be DEAD. Over-sensitive beats dead.
#[must_use]
pub fn scroll_threshold(unit: gdk::ScrollUnit, cell_h: f64) -> f64 {
    match unit {
        // A SURFACE delta is in pixels, so one cell height is one line step — but
        // only when that cell height is a usable number; otherwise fall through to
        // the detent threshold rather than divide by garbage.
        gdk::ScrollUnit::Surface if cell_h.is_finite() && cell_h > 0.0 => cell_h,
        // `gdk::ScrollUnit` is `#[non_exhaustive]` and carries `__Unknown(i32)`, so
        // this arm is forced. Wheel, a degenerate SURFACE cell height, and any unit
        // this build does not know about all land here.
        _ => WHEEL_THRESHOLD,
    }
}

/// Accumulates fractional scroll deltas into discrete whole-threshold line steps.
///
/// The carried remainder is SIGNED and carries between calls indefinitely, so a
/// run of sub-threshold nudges (a trackpad) crosses a boundary exactly once, and a
/// direction reversal is handled symmetrically. Deciding WHEN that carry must be
/// thrown away is deliberately NOT this type's job: [`PaneScroll`] composes it
/// with the target grid and [`reset`](ScrollAccumulator::reset)s it on a grid
/// change (mirroring macOS's `if grid != lastScrollGrid { reset() }`).
///
/// `Copy` on purpose: `pane_view` stores it in a `Cell`, so no borrow is ever held
/// across a `wolf_core::WolfCore` call.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollAccumulator {
    /// The signed, carried sub-threshold remainder, in the delta's own unit.
    remainder: f64,
}

impl ScrollAccumulator {
    /// Creates a zeroed accumulator. Equivalent to `Self::default()`.
    #[must_use]
    pub const fn new() -> Self {
        Self { remainder: 0.0 }
    }

    /// Adds `delta` to the carried remainder and returns the signed count of whole
    /// `threshold`s newly crossed (positive in the `+delta` direction). The
    /// sub-threshold leftover carries to the next call.
    ///
    /// Guarantees the tests pin:
    ///
    /// 1. A non-finite `delta`, or a non-finite or non-positive `threshold`,
    ///    returns `0` and leaves the carried remainder UNTOUCHED — a rejected call
    ///    must never poison the accumulator (a NaN remainder would kill scrolling
    ///    for the rest of the session).
    /// 2. If adding `delta` makes the remainder itself non-finite, the remainder is
    ///    ZEROED and `0` is returned — recover, do not poison.
    /// 3. The returned count is clamped to
    ///    `[-MAX_STEPS_PER_EVENT, MAX_STEPS_PER_EVENT]`, and when the clamp bites,
    ///    the carry is DROPPED (a delta that large is garbage; carrying it would
    ///    keep firing forever).
    /// 4. It is O(1). **Deliberate divergence from macOS:**
    ///    `ScrollAccumulator.swift` uses two `while` loops that are
    ///    `O(delta / threshold)` and would effectively hang on a pathological
    ///    delta. The arithmetic form here has identical semantics for every
    ///    realistic input — `scroll_accumulator_table` pins that equivalence, and
    ///    its `1e300` case would never finish under the loop form.
    pub fn consume(&mut self, delta: f64, threshold: f64) -> i32 {
        // (1) Reject garbage BEFORE touching the carry: a rejected call must be
        // indistinguishable, from the accumulator's point of view, from no call.
        if !delta.is_finite() || !threshold.is_finite() || threshold <= 0.0 {
            return 0;
        }

        // (2) Accumulate. Two finite deltas can still sum to an infinity, in which
        // case the carry is unrecoverable — zero it rather than let a poisoned
        // remainder kill scrolling for the rest of the session.
        self.remainder += delta;
        if !self.remainder.is_finite() {
            self.remainder = 0.0;
            return 0;
        }

        // (3) Truncate TOWARD ZERO, which is what makes the reversal case
        // symmetric (and is semantically identical to the macOS two-`while`-loop
        // form, without its O(delta / threshold) cost).
        let raw = (self.remainder / threshold).trunc();
        // (4) Clamp in f64 space, BEFORE the cast, so the cast is exact and in
        // range by construction.
        let clamped = raw.clamp(-MAX_STEPS_PER_EVENT_F, MAX_STEPS_PER_EVENT_F);

        // (5) Carry the sub-threshold leftover — unless the clamp bit, in which
        // case the leftover is garbage and carrying it would keep firing forever.
        self.remainder -= clamped * threshold;
        if clamped != raw {
            self.remainder = 0.0;
        }

        // (6) `clamped` is in [-256.0, 256.0] and has no fractional part.
        #[allow(
            clippy::cast_possible_truncation,
            reason = "clamped in f64 space to [-MAX_STEPS_PER_EVENT_F, MAX_STEPS_PER_EVENT_F] and truncated, so the cast is exact and in range"
        )]
        {
            clamped as i32
        }
    }

    /// Clears the carried remainder.
    ///
    /// Called only when the scroll target grid changes, so a leftover from one
    /// pane cannot nudge the next one — see [`PaneScroll`], which owns that rule.
    pub fn reset(&mut self) {
        self.remainder = 0.0;
    }
}

/// Per-pane scroll state: the accumulator plus the grid it was last applied to.
///
/// This is the whole of `pane_view`'s scroll bookkeeping, expressed as a pure
/// value. The rule it exists for — **a change of target grid discards the carried
/// sub-line remainder** — is the macOS `if grid != lastScrollGrid { reset() }`,
/// which on macOS lives in the VIEW (`PaneContainerView.scrollWheel`). Keeping it
/// there on this side would put it in `pane_view.rs`, which needs a live display,
/// so nothing could execute it: pointer input has NO end-to-end coverage on Linux
/// (`crates/wolf-gnome/tests/README.md`, "Deliberate test gaps"), and a rule left
/// in the GTK glue is therefore a rule nothing verifies at all. Here it is
/// decidable from the arguments, and `pane_scroll_table` /
/// `grid_change_discards_the_carry` pin it headlessly.
///
/// The grid id is used ONLY for equality against the previous call; this type
/// never resolves it against a snapshot.
///
/// `Copy` for the same reason [`ScrollAccumulator`] is: `pane_view` stores it in
/// a `Cell`, so no borrow is ever held across a `wolf_core::WolfCore` call.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PaneScroll {
    /// The carried sub-threshold remainder, and every guard that goes with it.
    acc: ScrollAccumulator,
    /// The grid named by the PREVIOUS [`PaneScroll::steps_for`] call, or `None`
    /// before the first one.
    last_grid: Option<u64>,
}

impl PaneScroll {
    /// Creates a zeroed state: no carry, and no previously-seen grid.
    ///
    /// Equivalent to `Self::default()`, which `pane_scroll_new_matches_default`
    /// pins. (The `Default` derive is also what satisfies
    /// `clippy::new_without_default` for this inherent `new`.)
    #[must_use]
    pub const fn new() -> Self {
        Self {
            acc: ScrollAccumulator::new(),
            last_grid: None,
        }
    }

    /// Accumulates `delta` against `threshold` for `grid`, resetting the carried
    /// remainder FIRST if `grid` differs from the grid of the previous call, and
    /// returns the signed step count exactly as [`ScrollAccumulator::consume`]
    /// does.
    ///
    /// Why the reset: the remainder is a fraction of a LINE **of one particular
    /// grid**. Carrying half a line out of the pane the pointer just left and
    /// into the pane it just entered would make the new pane's first event jump
    /// by a step it did not earn.
    ///
    /// "The previous call" means the previous call to THIS method, accepted or
    /// not: a call that `consume` rejects (a non-finite delta, or a non-positive
    /// or non-finite threshold) still names a grid, and still records it. So a
    /// rejected call on a different grid ends the old grid's carry just like an
    /// accepted one would — pinned by `pane_scroll_table`'s "a rejected call
    /// still counts as a call" case.
    ///
    /// Everything else is [`ScrollAccumulator`]'s contract verbatim — the
    /// non-finite guards, the O(1) arithmetic, the `±MAX_STEPS_PER_EVENT` clamp
    /// and the carry are sequenced here, never re-implemented.
    pub fn steps_for(&mut self, grid: u64, delta: f64, threshold: f64) -> i32 {
        // Record the grid BEFORE `consume` runs and UNCONDITIONALLY, so a call
        // `consume` goes on to reject still counts as "the previous call" for the
        // next one. That is not an accident of the ordering: a rejected call named
        // a grid, and the carry it would have been added to belongs to a DIFFERENT
        // grid, so keeping the old grid recorded would let that carry leak forward.
        if self.last_grid != Some(grid) {
            self.acc.reset();
            self.last_grid = Some(grid);
        }
        // Every other rule — the non-finite guards, the O(1) arithmetic, the clamp
        // and the carry — is `ScrollAccumulator`'s, sequenced here and never
        // re-implemented.
        self.acc.consume(delta, threshold)
    }
}

/// Encodes a signed step count from [`ScrollAccumulator::consume`] as a
/// `wolf_core::ScrollDirection` plus a POSITIVE repeat count, or `None` when there
/// is nothing to send.
///
/// `0` yields `None`; `steps > 0` yields `Down`; `steps < 0` yields `Up`. The
/// magnitude is `steps.unsigned_abs()`, which is total — a plain `-steps` would
/// PANIC in debug builds on `i32::MIN`.
///
/// # THIS SIGN CONVENTION IS THE OPPOSITE OF macOS'S. DO NOT "FIX" IT.
///
/// AppKit's `scrollingDeltaY > 0` means "show earlier content" (the viewport moves
/// toward line 1), so `crates/wolf-macos` maps a positive step count to `Up`.
/// GDK's smooth-scroll `dy > 0` corresponds to `GDK_SCROLL_DOWN` — the viewport
/// moves toward the END of the buffer — so GTK maps a positive step count to
/// `Down`. And `ScrollDirection::Down` is empirically pinned (by
/// `crates/wolf-ffi/tests/mouse_scroll.rs`, against real Neovim) to strictly
/// INCREASE `win_viewport.topline`, which is exactly what a positive GDK `dy`
/// should do.
///
/// **If scrolling feels inverted, flip THIS line** — and then fix
/// `scroll_direction_is_inverted_from_macos`, which exists solely to stop this
/// mapping from being "corrected" back while cross-reading
/// `PaneContainerView.swift:807`.
#[must_use]
pub fn scroll_steps(steps: i32) -> Option<(wolf_core::ScrollDirection, u32)> {
    // GDK dy > 0 is GDK_SCROLL_DOWN. This is the INVERSE of the macOS mapping and
    // is correct — read the doc comment above before touching it.
    let direction = match steps {
        0 => return None,
        s if s > 0 => wolf_core::ScrollDirection::Down,
        _ => wolf_core::ScrollDirection::Up,
    };
    // `unsigned_abs` is total: `-i32::MIN` would panic in a debug build.
    Some((direction, steps.unsigned_abs()))
}

#[cfg(test)]
mod tests {
    use super::*;

    use gdk::ModifierType;
    use wolf_core::{MouseButton, ScrollDirection};

    // -- mouse_input --------------------------------------------------------

    /// A GDK button number of 0 — no button. Never forwarded.
    const BUTTON_NONE: u32 = 0;
    /// The "back" thumb button GDK numbers 8. Never forwarded.
    const BUTTON_BACK: u32 = 8;
    /// The "forward" thumb button GDK numbers 9. Never forwarded.
    const BUTTON_FORWARD: u32 = 9;

    /// One `mouse_input` table row.
    struct MouseCase {
        desc: &'static str,
        button: u32,
        state: ModifierType,
        expected: Option<MouseInput>,
    }

    /// The expected `Some(MouseInput)` for the left button with the given
    /// modifiers. Written as a helper so a case row reads as its three modifier
    /// bits and nothing else.
    fn left(ctrl: bool, shift: bool, alt: bool) -> Option<MouseInput> {
        Some(MouseInput {
            button: MouseButton::Left,
            ctrl,
            shift,
            alt,
        })
    }

    #[test]
    fn mouse_input_table() {
        let cases = [
            // --- The primary button, with every modifier combination we honor ---
            MouseCase {
                desc: "primary button, no modifiers",
                button: gdk::BUTTON_PRIMARY,
                state: ModifierType::empty(),
                expected: left(false, false, false),
            },
            MouseCase {
                desc: "primary button + Control",
                button: gdk::BUTTON_PRIMARY,
                state: ModifierType::CONTROL_MASK,
                expected: left(true, false, false),
            },
            MouseCase {
                desc: "primary button + Shift",
                button: gdk::BUTTON_PRIMARY,
                state: ModifierType::SHIFT_MASK,
                expected: left(false, true, false),
            },
            MouseCase {
                desc: "primary button + Alt",
                button: gdk::BUTTON_PRIMARY,
                state: ModifierType::ALT_MASK,
                expected: left(false, false, true),
            },
            MouseCase {
                desc: "primary button + Control + Shift + Alt",
                button: gdk::BUTTON_PRIMARY,
                state: ModifierType::CONTROL_MASK
                    .union(ModifierType::SHIFT_MASK)
                    .union(ModifierType::ALT_MASK),
                expected: left(true, true, true),
            },
            // --- Modifier bits OUTSIDE the nvim mouse grammar are ignored -------
            // Neovim has no meta mouse modifier, so Super must not leak through as
            // anything; Caps Lock is not a modifier the grammar knows either.
            MouseCase {
                desc: "primary button + Super: Super is IGNORED, not forwarded",
                button: gdk::BUTTON_PRIMARY,
                state: ModifierType::SUPER_MASK,
                expected: left(false, false, false),
            },
            MouseCase {
                desc: "primary button + Super + Control: only ctrl survives",
                button: gdk::BUTTON_PRIMARY,
                state: ModifierType::SUPER_MASK.union(ModifierType::CONTROL_MASK),
                expected: left(true, false, false),
            },
            MouseCase {
                desc: "primary button + Caps Lock: lock bits are ignored",
                button: gdk::BUTTON_PRIMARY,
                state: ModifierType::LOCK_MASK,
                expected: left(false, false, false),
            },
            // --- Every non-primary button is REFUSED ---------------------------
            // Deliberate parity with macOS: no right-click menu, no middle-click
            // paste, on either platform.
            MouseCase {
                desc: "middle button is refused",
                button: gdk::BUTTON_MIDDLE,
                state: ModifierType::empty(),
                expected: None,
            },
            MouseCase {
                desc: "secondary button is refused",
                button: gdk::BUTTON_SECONDARY,
                state: ModifierType::empty(),
                expected: None,
            },
            MouseCase {
                desc: "button 0 (no button) is refused",
                button: BUTTON_NONE,
                state: ModifierType::empty(),
                expected: None,
            },
            MouseCase {
                desc: "thumb button 8 (back) is refused",
                button: BUTTON_BACK,
                state: ModifierType::empty(),
                expected: None,
            },
            MouseCase {
                desc: "thumb button 9 (forward) is refused",
                button: BUTTON_FORWARD,
                state: ModifierType::empty(),
                expected: None,
            },
            // A refused button stays refused no matter what is held down.
            MouseCase {
                desc: "secondary button + Control is still refused",
                button: gdk::BUTTON_SECONDARY,
                state: ModifierType::CONTROL_MASK,
                expected: None,
            },
        ];

        for case in &cases {
            let got = mouse_input(case.button, case.state);
            assert_eq!(got, case.expected, "case: {}", case.desc);
        }
    }

    // -- scroll_threshold ---------------------------------------------------

    /// A "typical" monospace cell height, in pixels — the SURFACE-unit threshold.
    const CELL_H: f64 = 16.0;

    /// An arbitrary payload for `gdk::ScrollUnit::__Unknown`, standing in for a
    /// unit a future GDK adds that this build does not know about.
    const UNKNOWN_UNIT_CODE: i32 = 7;

    /// One `scroll_threshold` table row.
    struct ThresholdCase {
        desc: &'static str,
        unit: gdk::ScrollUnit,
        cell_h: f64,
        expected: f64,
    }

    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "every expected value here is either WHEEL_THRESHOLD (1.0) or CELL_H (16.0) — \
                  both exactly representable in binary f64 — and the seam returns the value \
                  unmodified rather than computing with it, so an epsilon comparison would \
                  weaken the assertion without making it more correct"
    )]
    fn scroll_threshold_table() {
        let cases = [
            // --- WHEEL: one detent is one step, whatever the cell height is -----
            ThresholdCase {
                desc: "Wheel with a normal cell height -> one detent",
                unit: gdk::ScrollUnit::Wheel,
                cell_h: CELL_H,
                expected: WHEEL_THRESHOLD,
            },
            ThresholdCase {
                desc: "Wheel ignores a degenerate cell height entirely",
                unit: gdk::ScrollUnit::Wheel,
                cell_h: 0.0,
                expected: WHEEL_THRESHOLD,
            },
            // --- SURFACE: a pixel delta, so one cell height is one step ---------
            ThresholdCase {
                desc: "Surface with a normal cell height -> that cell height",
                unit: gdk::ScrollUnit::Surface,
                cell_h: CELL_H,
                expected: CELL_H,
            },
            // --- SURFACE with a degenerate cell height falls back to a detent ---
            ThresholdCase {
                desc: "Surface with cell_h == 0.0 falls back to the detent threshold",
                unit: gdk::ScrollUnit::Surface,
                cell_h: 0.0,
                expected: WHEEL_THRESHOLD,
            },
            ThresholdCase {
                desc: "Surface with a negative cell_h falls back to the detent threshold",
                unit: gdk::ScrollUnit::Surface,
                cell_h: -3.0,
                expected: WHEEL_THRESHOLD,
            },
            ThresholdCase {
                desc: "Surface with a NaN cell_h falls back to the detent threshold",
                unit: gdk::ScrollUnit::Surface,
                cell_h: f64::NAN,
                expected: WHEEL_THRESHOLD,
            },
            ThresholdCase {
                desc: "Surface with an infinite cell_h falls back to the detent threshold",
                unit: gdk::ScrollUnit::Surface,
                cell_h: f64::INFINITY,
                expected: WHEEL_THRESHOLD,
            },
            // --- The `#[non_exhaustive]` catch-all: over-sensitive beats dead ---
            ThresholdCase {
                desc: "an unknown scroll unit falls back to the detent threshold",
                unit: gdk::ScrollUnit::__Unknown(UNKNOWN_UNIT_CODE),
                cell_h: CELL_H,
                expected: WHEEL_THRESHOLD,
            },
        ];

        for case in &cases {
            let got = scroll_threshold(case.unit, case.cell_h);
            assert_eq!(got, case.expected, "case: {}", case.desc);
        }
    }

    // -- ScrollAccumulator --------------------------------------------------

    /// Half a cell, in pixels.
    const HALF_CELL: f64 = 8.0;
    /// A cell and a half, in pixels: one whole step plus a half-cell carry.
    const CELL_AND_A_HALF: f64 = 24.0;
    /// Two whole cells, in pixels.
    const TWO_CELLS: f64 = 32.0;
    /// A sub-cell trackpad nudge (0.6 of a cell), so two of them cross exactly one
    /// boundary and leave a carry.
    const SUB_CELL: f64 = 9.6;
    /// One WHEEL detent, used as a threshold.
    const DETENT: f64 = 1.0;
    /// A delta so large the step count must saturate at `MAX_STEPS_PER_EVENT`. Under
    /// the macOS `while`-loop formulation this single call would iterate 1e300
    /// times, i.e. never finish — so this row is also the O(1) regression guard.
    const ENORMOUS: f64 = 1e300;
    /// Roughly half of `f64::MAX`. Two of these overflow the CARRIED REMAINDER to
    /// infinity, which is the only way to reach `consume`'s second guard using
    /// finite deltas.
    const HALF_OF_F64_MAX: f64 = 1e308;
    /// The negative saturation bound, `-MAX_STEPS_PER_EVENT`.
    const MIN_STEPS_PER_EVENT: i32 = -MAX_STEPS_PER_EVENT;

    /// One step in an accumulator scenario. A case applies its ops IN ORDER to a
    /// single fresh accumulator, which is what makes the carry observable.
    enum Op {
        /// `consume(delta, threshold)` must return exactly `expect`.
        Consume {
            delta: f64,
            threshold: f64,
            expect: i32,
        },
        /// `reset()` — clears the carried remainder.
        Reset,
    }

    /// One `ScrollAccumulator` scenario.
    struct AccCase {
        desc: &'static str,
        ops: &'static [Op],
    }

    /// The accumulator scenarios, as data.
    ///
    /// A `const` item (rather than a local `let` array) because the nested
    /// `&'static [Op]` slices need a `'static` allocation — the same shape
    /// `actions::MENU_SECTIONS` uses for its nested item slices.
    const ACC_CASES: &[AccCase] = &[
        // --- The macOS-ported behaviours ------------------------------------
        AccCase {
            desc: "one whole cell crosses exactly one step",
            ops: &[Op::Consume {
                delta: CELL_H,
                threshold: CELL_H,
                expect: 1,
            }],
        },
        AccCase {
            desc: "sub-cell nudges carry: 0.6 + 0.6 of a cell crosses once",
            ops: &[
                Op::Consume {
                    delta: SUB_CELL,
                    threshold: CELL_H,
                    expect: 0,
                },
                Op::Consume {
                    delta: SUB_CELL,
                    threshold: CELL_H,
                    expect: 1,
                },
            ],
        },
        AccCase {
            desc: "the leftover of a 1.5-cell delta carries into the next call",
            ops: &[
                Op::Consume {
                    delta: CELL_AND_A_HALF,
                    threshold: CELL_H,
                    expect: 1,
                },
                Op::Consume {
                    delta: HALF_CELL,
                    threshold: CELL_H,
                    expect: 1,
                },
            ],
        },
        AccCase {
            desc: "a direction reversal is handled symmetrically",
            ops: &[
                Op::Consume {
                    delta: CELL_AND_A_HALF,
                    threshold: CELL_H,
                    expect: 1,
                },
                Op::Consume {
                    delta: -TWO_CELLS,
                    threshold: CELL_H,
                    expect: -1,
                },
            ],
        },
        AccCase {
            desc: "reset() clears the carry, so the next nudge starts from zero",
            ops: &[
                Op::Consume {
                    delta: SUB_CELL,
                    threshold: CELL_H,
                    expect: 0,
                },
                Op::Reset,
                Op::Consume {
                    delta: SUB_CELL,
                    threshold: CELL_H,
                    expect: 0,
                },
            ],
        },
        AccCase {
            desc: "exactly one cell three times running: one step each, no drift",
            ops: &[
                Op::Consume {
                    delta: CELL_H,
                    threshold: CELL_H,
                    expect: 1,
                },
                Op::Consume {
                    delta: CELL_H,
                    threshold: CELL_H,
                    expect: 1,
                },
                Op::Consume {
                    delta: CELL_H,
                    threshold: CELL_H,
                    expect: 1,
                },
            ],
        },
        AccCase {
            desc: "a detent threshold yields exactly one step per detent",
            ops: &[
                Op::Consume {
                    delta: DETENT,
                    threshold: DETENT,
                    expect: 1,
                },
                Op::Consume {
                    delta: DETENT,
                    threshold: DETENT,
                    expect: 1,
                },
                Op::Consume {
                    delta: DETENT,
                    threshold: DETENT,
                    expect: 1,
                },
            ],
        },
        // --- GTK-specific hardening: guards macOS does not have --------------
        AccCase {
            desc: "a zero threshold returns 0, does not hang, and does not break the next call",
            ops: &[
                Op::Consume {
                    delta: CELL_H,
                    threshold: 0.0,
                    expect: 0,
                },
                Op::Consume {
                    delta: CELL_H,
                    threshold: CELL_H,
                    expect: 1,
                },
            ],
        },
        AccCase {
            desc: "negative / NaN / infinite thresholds return 0 and leave the carry UNTOUCHED",
            ops: &[
                // Establish a half-boundary carry first...
                Op::Consume {
                    delta: SUB_CELL,
                    threshold: CELL_H,
                    expect: 0,
                },
                Op::Consume {
                    delta: CELL_H,
                    threshold: -CELL_H,
                    expect: 0,
                },
                Op::Consume {
                    delta: CELL_H,
                    threshold: f64::NAN,
                    expect: 0,
                },
                Op::Consume {
                    delta: CELL_H,
                    threshold: f64::INFINITY,
                    expect: 0,
                },
                // ...and prove it survived all three rejected calls: the carry
                // plus one more nudge still crosses exactly one boundary.
                Op::Consume {
                    delta: SUB_CELL,
                    threshold: CELL_H,
                    expect: 1,
                },
            ],
        },
        AccCase {
            desc: "a non-finite delta returns 0 WITHOUT poisoning the carried remainder",
            ops: &[
                Op::Consume {
                    delta: f64::NAN,
                    threshold: CELL_H,
                    expect: 0,
                },
                Op::Consume {
                    delta: f64::INFINITY,
                    threshold: CELL_H,
                    expect: 0,
                },
                Op::Consume {
                    delta: f64::NEG_INFINITY,
                    threshold: CELL_H,
                    expect: 0,
                },
                // If any of the three had been added to the remainder, it would
                // now be NaN and this would return 0 forever.
                Op::Consume {
                    delta: CELL_H,
                    threshold: CELL_H,
                    expect: 1,
                },
            ],
        },
        AccCase {
            desc: "a remainder that overflows to infinity is zeroed, not poisoned",
            ops: &[
                // Both deltas are finite and each on its own crosses nothing,
                // so the ONLY way the remainder goes non-finite is the sum.
                Op::Consume {
                    delta: HALF_OF_F64_MAX,
                    threshold: f64::MAX,
                    expect: 0,
                },
                Op::Consume {
                    delta: HALF_OF_F64_MAX,
                    threshold: f64::MAX,
                    expect: 0,
                },
                Op::Consume {
                    delta: f64::MAX,
                    threshold: f64::MAX,
                    expect: 1,
                },
            ],
        },
        AccCase {
            desc: "an enormous delta saturates at MAX_STEPS_PER_EVENT and returns instantly",
            ops: &[Op::Consume {
                delta: ENORMOUS,
                threshold: DETENT,
                expect: MAX_STEPS_PER_EVENT,
            }],
        },
        AccCase {
            desc: "an enormous NEGATIVE delta saturates at -MAX_STEPS_PER_EVENT",
            ops: &[Op::Consume {
                delta: -ENORMOUS,
                threshold: DETENT,
                expect: MIN_STEPS_PER_EVENT,
            }],
        },
        AccCase {
            desc: "a clamped delta DROPS the carry, so it cannot keep firing forever",
            ops: &[
                Op::Consume {
                    delta: ENORMOUS,
                    threshold: DETENT,
                    expect: MAX_STEPS_PER_EVENT,
                },
                Op::Consume {
                    delta: 0.0,
                    threshold: DETENT,
                    expect: 0,
                },
            ],
        },
    ];

    /// Every scenario in [`ACC_CASES`], each against its own fresh accumulator.
    ///
    /// The `1e300` rows also assert a TIMING property that no `assert!` can state:
    /// under the macOS `while`-loop formulation those calls would iterate ~1e300
    /// times and this test would never return. If this test hangs, the arithmetic
    /// form has been replaced by a loop.
    #[test]
    fn scroll_accumulator_table() {
        for case in ACC_CASES {
            let mut acc = ScrollAccumulator::new();
            for (i, op) in case.ops.iter().enumerate() {
                match *op {
                    Op::Consume {
                        delta,
                        threshold,
                        expect,
                    } => {
                        let got = acc.consume(delta, threshold);
                        assert_eq!(
                            got, expect,
                            "case {:?}, op #{i}: consume(delta={delta}, threshold={threshold})",
                            case.desc
                        );
                    }
                    Op::Reset => acc.reset(),
                }
            }
        }
    }

    /// A fresh accumulator carries nothing, and equals `Default::default()`.
    #[test]
    fn scroll_accumulator_new_matches_default() {
        assert_eq!(
            ScrollAccumulator::new(),
            ScrollAccumulator::default(),
            "`new()` and `Default::default()` must agree. BOTH spellings are live: the `Default` \
             derive is what satisfies `clippy::new_without_default` for the inherent `new()`, and \
             `PaneScroll` reaches this type through each of them — `PaneScroll::new()` builds it \
             with `new()`, `PaneScroll::default()` builds it through the derive — so a \
             disagreement would give a pane a phantom carry depending on which constructor built \
             it"
        );
    }

    // -- PaneScroll ---------------------------------------------------------

    /// A window grid id. Any two DISTINCT ids would exercise the rule; these sit
    /// in the real window-grid range (grid 1 is the outer/global grid, editor
    /// windows draw into grids `>= 2` under `ext_multigrid`), so the scenarios
    /// read like the events `pane_view` actually forwards.
    const GRID_A: u64 = 2;
    /// A second, DIFFERENT window grid — the pane the pointer moves to.
    const GRID_B: u64 = 3;
    /// A third window grid, so a scenario can change grid twice without going
    /// back to one it has already used.
    const GRID_C: u64 = 4;

    /// One step in a [`PaneScroll`] scenario: `steps_for(grid, delta, threshold)`
    /// must return exactly `expect`.
    ///
    /// There is deliberately no `Reset` op, unlike [`Op`]: `PaneScroll` exposes no
    /// manual reset at all, and a grid CHANGE being the only way to clear the
    /// carry is precisely the property under test.
    struct PaneOp {
        grid: u64,
        delta: f64,
        threshold: f64,
        expect: i32,
    }

    /// One `PaneScroll` scenario. Its ops apply IN ORDER to a single fresh
    /// `PaneScroll`, which is what makes the carry — and, more importantly, its
    /// LOSS — observable.
    struct PaneCase {
        desc: &'static str,
        ops: &'static [PaneOp],
    }

    /// The `PaneScroll` scenarios, as data (a `const` for the same reason
    /// [`ACC_CASES`] is one: the nested slices need a `'static` allocation).
    const PANE_CASES: &[PaneCase] = &[
        // --- Nothing is reset that should not be ----------------------------
        PaneCase {
            desc: "the very first call on a fresh PaneScroll is honoured in full",
            ops: &[PaneOp {
                grid: GRID_A,
                delta: CELL_H,
                threshold: CELL_H,
                expect: 1,
            }],
        },
        PaneCase {
            desc: "the SAME grid carries: 0.6 + 0.6 of a cell crosses exactly once",
            ops: &[
                PaneOp {
                    grid: GRID_A,
                    delta: SUB_CELL,
                    threshold: CELL_H,
                    expect: 0,
                },
                // If `PaneScroll` reset on every call — or treated the fresh
                // `last_grid: None` as a permanent mismatch — this would be 0.
                PaneOp {
                    grid: GRID_A,
                    delta: SUB_CELL,
                    threshold: CELL_H,
                    expect: 1,
                },
            ],
        },
        PaneCase {
            desc: "exactly one cell three times running on one grid: one step each, no drift",
            ops: &[
                PaneOp {
                    grid: GRID_A,
                    delta: CELL_H,
                    threshold: CELL_H,
                    expect: 1,
                },
                PaneOp {
                    grid: GRID_A,
                    delta: CELL_H,
                    threshold: CELL_H,
                    expect: 1,
                },
                PaneOp {
                    grid: GRID_A,
                    delta: CELL_H,
                    threshold: CELL_H,
                    expect: 1,
                },
            ],
        },
        PaneCase {
            desc: "a direction REVERSAL on one grid is not a grid change: symmetric, no reset",
            ops: &[
                PaneOp {
                    grid: GRID_A,
                    delta: CELL_AND_A_HALF,
                    threshold: CELL_H,
                    expect: 1,
                },
                // The 0.5-cell carry must survive the sign flip, exactly as in
                // `ScrollAccumulator`'s own reversal row.
                PaneOp {
                    grid: GRID_A,
                    delta: -TWO_CELLS,
                    threshold: CELL_H,
                    expect: -1,
                },
            ],
        },
        // --- THE RULE: a grid change discards the carry ----------------------
        PaneCase {
            desc: "a GRID CHANGE discards the carry (the load-bearing case)",
            ops: &[
                PaneOp {
                    grid: GRID_A,
                    delta: SUB_CELL,
                    threshold: CELL_H,
                    expect: 0,
                },
                // WITHOUT the reset this is 1: 0.6 + 0.6 of a cell crosses a
                // boundary. With it, grid B starts from zero and crosses nothing.
                PaneOp {
                    grid: GRID_B,
                    delta: SUB_CELL,
                    threshold: CELL_H,
                    expect: 0,
                },
            ],
        },
        PaneCase {
            desc: "changing BACK also resets: the rule is 'differs from the previous call', \
                   not 'never seen before'",
            ops: &[
                PaneOp {
                    grid: GRID_A,
                    delta: SUB_CELL,
                    threshold: CELL_H,
                    expect: 0,
                },
                PaneOp {
                    grid: GRID_B,
                    delta: SUB_CELL,
                    threshold: CELL_H,
                    expect: 0,
                },
                // Grid A has been seen before, but not LAST — so its old carry is
                // long gone and this starts from zero too.
                PaneOp {
                    grid: GRID_A,
                    delta: SUB_CELL,
                    threshold: CELL_H,
                    expect: 0,
                },
                // ...and the resets must not have BROKEN the accumulator: two
                // nudges on one grid still cross exactly one boundary.
                PaneOp {
                    grid: GRID_A,
                    delta: SUB_CELL,
                    threshold: CELL_H,
                    expect: 1,
                },
            ],
        },
        PaneCase {
            desc: "the leftover a WHOLE-step call parks is dropped by a grid change too",
            ops: &[
                // Crosses one boundary and parks a half-cell leftover.
                PaneOp {
                    grid: GRID_A,
                    delta: CELL_AND_A_HALF,
                    threshold: CELL_H,
                    expect: 1,
                },
                // That leftover plus this half-cell would be exactly one step if
                // it had carried across the grid change. It must not have.
                PaneOp {
                    grid: GRID_C,
                    delta: HALF_CELL,
                    threshold: CELL_H,
                    expect: 0,
                },
            ],
        },
        PaneCase {
            desc: "a rejected call still counts as a call for the grid-change rule",
            ops: &[
                PaneOp {
                    grid: GRID_A,
                    delta: SUB_CELL,
                    threshold: CELL_H,
                    expect: 0,
                },
                // `consume` REFUSES this NaN delta — but the call still names grid
                // B, so it still ends grid A's carry.
                PaneOp {
                    grid: GRID_B,
                    delta: f64::NAN,
                    threshold: CELL_H,
                    expect: 0,
                },
                // Therefore this is a change back to A from zero, not the second
                // half of A's original 0.6 + 0.6.
                PaneOp {
                    grid: GRID_A,
                    delta: SUB_CELL,
                    threshold: CELL_H,
                    expect: 0,
                },
            ],
        },
        // --- The guards pass straight through ScrollAccumulator --------------
        PaneCase {
            desc: "zero / negative / NaN / infinite thresholds return 0 and leave the carry \
                   UNTOUCHED",
            ops: &[
                // Establish a half-boundary carry first...
                PaneOp {
                    grid: GRID_A,
                    delta: SUB_CELL,
                    threshold: CELL_H,
                    expect: 0,
                },
                PaneOp {
                    grid: GRID_A,
                    delta: CELL_H,
                    threshold: 0.0,
                    expect: 0,
                },
                PaneOp {
                    grid: GRID_A,
                    delta: CELL_H,
                    threshold: -CELL_H,
                    expect: 0,
                },
                PaneOp {
                    grid: GRID_A,
                    delta: CELL_H,
                    threshold: f64::NAN,
                    expect: 0,
                },
                PaneOp {
                    grid: GRID_A,
                    delta: CELL_H,
                    threshold: f64::INFINITY,
                    expect: 0,
                },
                // ...and prove it survived all four rejected calls.
                PaneOp {
                    grid: GRID_A,
                    delta: SUB_CELL,
                    threshold: CELL_H,
                    expect: 1,
                },
            ],
        },
        PaneCase {
            desc: "a non-finite delta returns 0 WITHOUT poisoning the carry",
            ops: &[
                PaneOp {
                    grid: GRID_A,
                    delta: f64::NAN,
                    threshold: CELL_H,
                    expect: 0,
                },
                PaneOp {
                    grid: GRID_A,
                    delta: f64::INFINITY,
                    threshold: CELL_H,
                    expect: 0,
                },
                PaneOp {
                    grid: GRID_A,
                    delta: f64::NEG_INFINITY,
                    threshold: CELL_H,
                    expect: 0,
                },
                // If any of the three had reached the remainder it would now be
                // NaN, and this would return 0 forever.
                PaneOp {
                    grid: GRID_A,
                    delta: CELL_H,
                    threshold: CELL_H,
                    expect: 1,
                },
            ],
        },
        PaneCase {
            desc: "the ±MAX_STEPS_PER_EVENT clamp is the accumulator's, unchanged",
            ops: &[PaneOp {
                grid: GRID_A,
                delta: ENORMOUS,
                threshold: DETENT,
                expect: MAX_STEPS_PER_EVENT,
            }],
        },
    ];

    /// Every scenario in [`PANE_CASES`], each against its own fresh `PaneScroll`.
    #[test]
    fn pane_scroll_table() {
        for case in PANE_CASES {
            let mut pane = PaneScroll::new();
            for (i, op) in case.ops.iter().enumerate() {
                let got = pane.steps_for(op.grid, op.delta, op.threshold);
                assert_eq!(
                    got, op.expect,
                    "case {:?}, op #{i}: steps_for(grid={}, delta={}, threshold={})",
                    case.desc, op.grid, op.delta, op.threshold
                );
            }
        }
    }

    // =========================================================================
    // The rule `PaneScroll` exists for, stated on its own so a failure NAMES it
    // rather than pointing at a table row.
    //
    // It used to live in `pane_view.rs::on_pane_scroll` — GTK glue, which needs a
    // live display, so nothing in the suite could execute it. That is the whole
    // reason it moved: pointer input has NO end-to-end coverage on Linux (see
    // `crates/wolf-gnome/tests/README.md`, "Deliberate test gaps"), so a rule left
    // in the glue is a rule NOTHING verifies. If this test is deleted along with
    // the type, the rule silently becomes unverified again.
    // =========================================================================
    #[test]
    fn grid_change_discards_the_carry() {
        let mut pane = PaneScroll::new();

        assert_eq!(
            pane.steps_for(GRID_A, SUB_CELL, CELL_H),
            0,
            "0.6 of a cell crosses no boundary on its own, so this call must only park a carry"
        );
        assert_eq!(
            pane.steps_for(GRID_B, SUB_CELL, CELL_H),
            0,
            "the second 0.6-cell nudge lands on grid {GRID_B}, NOT grid {GRID_A}, so the carry \
             from grid {GRID_A} must be discarded rather than completing a step here. A returned \
             1 means the reset-on-grid-change rule is gone and half a line of one pane's scroll \
             is leaking into the pane the pointer just entered"
        );
        assert_eq!(
            pane.steps_for(GRID_B, SUB_CELL, CELL_H),
            1,
            "...and the reset must not have BROKEN the accumulator: two nudges on grid \
             {GRID_B} still cross exactly one boundary"
        );
    }

    /// A fresh `PaneScroll` has no carry AND no previously-seen grid, and equals
    /// `Default::default()`.
    #[test]
    fn pane_scroll_new_matches_default() {
        assert_eq!(
            PaneScroll::new(),
            PaneScroll::default(),
            "`new()` and `Default::default()` must agree. `pane_view` constructs one per pane \
             with `new()`, while the `Default` derive is what satisfies \
             `clippy::new_without_default` — a disagreement in EITHER field would matter: a \
             non-zero carry is a phantom step, and a non-`None` `last_grid` would make the \
             first real scroll event on that pane a spurious grid CHANGE"
        );
    }

    // -- scroll_steps -------------------------------------------------------

    /// An arbitrary multi-step magnitude, big enough that a sign error cannot be
    /// mistaken for a rounding difference.
    const SEVERAL: i32 = 7;

    /// `i32::MIN.unsigned_abs()`, spelled out so the table pins the NUMBER rather
    /// than restating the call. A plain `-i32::MIN` panics in debug builds.
    const I32_MIN_MAGNITUDE: u32 = 2_147_483_648;

    /// One `scroll_steps` table row.
    struct StepsCase {
        desc: &'static str,
        steps: i32,
        expected: Option<(ScrollDirection, u32)>,
    }

    #[test]
    fn scroll_steps_table() {
        let cases = [
            StepsCase {
                desc: "zero steps means there is nothing to send",
                steps: 0,
                expected: None,
            },
            StepsCase {
                desc: "one positive step is ONE Down (GDK dy > 0 is GDK_SCROLL_DOWN)",
                steps: 1,
                expected: Some((ScrollDirection::Down, 1)),
            },
            StepsCase {
                desc: "several positive steps are several Downs",
                steps: SEVERAL,
                expected: Some((ScrollDirection::Down, SEVERAL.unsigned_abs())),
            },
            StepsCase {
                desc: "one negative step is ONE Up",
                steps: -1,
                expected: Some((ScrollDirection::Up, 1)),
            },
            StepsCase {
                desc: "several negative steps are several Ups",
                steps: -SEVERAL,
                expected: Some((ScrollDirection::Up, SEVERAL.unsigned_abs())),
            },
            StepsCase {
                desc: "i32::MIN does not overflow: unsigned_abs is total where -x is not",
                steps: i32::MIN,
                expected: Some((ScrollDirection::Up, I32_MIN_MAGNITUDE)),
            },
        ];

        for case in &cases {
            let got = scroll_steps(case.steps);
            assert_eq!(got, case.expected, "case: {}", case.desc);
        }
    }

    // =========================================================================
    // READ THIS BEFORE CHANGING `scroll_steps`.
    //
    // The GTK scroll sign is INVERTED relative to macOS. This is not a bug and it
    // is not a copy/paste slip from the Swift:
    //
    //   * AppKit: `scrollingDeltaY > 0` means "show EARLIER content", so
    //     `crates/wolf-macos` maps a positive step count to ScrollDirection::Up.
    //   * GDK: a smooth-scroll `dy > 0` is GDK_SCROLL_DOWN — the viewport moves
    //     toward the END of the buffer — so `wolf-gnome` maps a positive step
    //     count to ScrollDirection::Down.
    //   * ScrollDirection::Down is pinned against REAL Neovim (by
    //     `crates/wolf-ffi/tests/mouse_scroll.rs`) to strictly INCREASE
    //     `win_viewport.topline`, which is precisely what a positive GDK `dy`
    //     must do.
    //
    // If you are here because you were cross-reading `PaneContainerView.swift:807`
    // and this looked backwards: it is SUPPOSED to look backwards. Flipping it
    // would make GTK scrolling run the wrong way while every macOS test stayed
    // green, because macOS never runs this code.
    // =========================================================================
    #[test]
    fn scroll_direction_is_inverted_from_macos() {
        assert_eq!(
            scroll_steps(1),
            Some((ScrollDirection::Down, 1)),
            "a POSITIVE GDK dy must scroll DOWN (toward the end of the buffer, increasing \
             win_viewport.topline). macOS maps positive to Up; GTK must not. See the comment \
             above this test."
        );
        assert_eq!(
            scroll_steps(-1),
            Some((ScrollDirection::Up, 1)),
            "a NEGATIVE GDK dy must scroll UP (toward line 1, decreasing win_viewport.topline). \
             See the comment above this test."
        );
    }
}
