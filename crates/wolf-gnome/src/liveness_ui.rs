//! The pure, GTK-free rulebook for CRASH/STALL RECOVERY in `wolf-gnome`.
//!
//! `wolf-core` reports the captive Neovim's health as a [`wolf_core::Liveness`]
//! (`Alive` / `NotResponding` / `Dead`) — a cheap lock-free atomic read, not an
//! RPC — and offers `WolfCore::restart_nvim()` plus the RPC-free Tier-1 open-file
//! mirror `WolfCore::open_files_for_tabpage()`. This module owns every DECISION
//! that follows from those values; the widget work of carrying them out lives in
//! [`crate::crash_overlay`] and [`crate::shell`]. Keeping the decisions here (data
//! in, data out, no GTK types) is exactly what lets them be unit-tested
//! headlessly, the same way [`crate::command_action`], [`crate::geometry`] and
//! [`crate::cursor`] keep theirs pure.
//!
//! Four decisions live here:
//!
//! 1. [`liveness_ui`] — the three-state mapping from a liveness reading (plus
//!    whether an automatic restart attempt is still allowed) to *what the overlay
//!    shows*, *whether to auto-restart*, and *whether to paint this wake at all*.
//! 2. [`overlay_message`] — the message string each overlay mode carries.
//! 3. [`restart_outcome`] — what to do after `WolfCore::restart_nvim()` returns,
//!    given that an `Ok(false)` is a no-op rather than a failure.
//! 4. [`reopen_order`] — the order the captured Tier-1 files are reopened in, with
//!    the ACTIVE path last so it lands focused.
//!
//! Plus [`restart_button_rect`], the derived geometry of the "Restart Neovim"
//! button inside the overlay — the single source of truth shared by the widget
//! builder (which realises it as size requests) and the cage smoke (which calls
//! this function to derive the pixel it clicks). See its docs.
//!
//! ## Totality is the contract
//!
//! [`liveness_ui`] and [`overlay_message`] are `match`es with **no `_` arm**, so
//! appending a variant to [`wolf_core::Liveness`] or to [`OverlayMode`] is an
//! `E0004` compile error here until it is classified. That is the same guard
//! [`crate::command_action::ui_action_for`] and `pane_view::apply_intents` rely
//! on, and it exists because a silently-mishandled liveness state looks exactly
//! like a working one until an editor freezes.

use std::path::{Path, PathBuf};

use wolf_core::Liveness;

/// What the full-bleed crash/stall overlay is showing.
///
/// `Crashed { retrying }` distinguishes the two very different things a user can
/// be looking at while the core is `Dead`: the automatic attempt budget still
/// has room, so a restart is coming without the user doing anything
/// (`retrying: true`), or the budget is spent and only the button is left
/// (`retrying: false`).
///
/// `retrying` is derived PURELY from the attempt budget
/// ([`MAX_AUTO_RESTART_ATTEMPTS`]) — it says an automatic attempt is still
/// PERMITTED, not that one is currently running. `shell::schedule_restart`
/// increments the counter when it schedules the attempt, i.e. BEFORE the attempt
/// runs, so a `Dead` reading taken while an automatic restart is in flight
/// reports `retrying: false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayMode {
    /// No overlay: the editor is usable and paints normally.
    Hidden,
    /// The captive Neovim died. `retrying` is whether an automatic restart
    /// attempt is still permitted.
    Crashed {
        /// Whether the attempt budget still allows an automatic restart. NOT
        /// "an attempt is in flight" — see the type's docs.
        retrying: bool,
    },
    /// The captive Neovim is wedged (a synchronous RPC elapsed its budget).
    Stalled,
}

/// Everything one liveness reading decides, as one value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LivenessUi {
    /// What the crash/stall overlay must show.
    pub overlay: OverlayMode,
    /// Whether the front-end should schedule an AUTOMATIC restart.
    pub auto_restart: bool,
    /// Whether the pane view and the file tree should run their render wake. A
    /// frozen last frame is hidden by the opaque overlay, so repainting it is
    /// pure waste — and a future front-end path that issues a synchronous RPC
    /// would stall the GTK main loop for the whole timeout budget.
    pub paint: bool,
}

/// What follows a `WolfCore::restart_nvim()` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartOutcome {
    /// An ACTUAL respawn happened (`Ok(true)`): reset the pane view, reopen the
    /// captured files, hide the overlay, flash the banner.
    Respawned,
    /// No respawn happened (`Ok(false)`, or an `Err`): re-apply the overlay from a
    /// FRESH liveness reading and do nothing else — no rebind, no reopen, no
    /// banner, no rescheduling.
    Refresh(LivenessUi),
}

/// The overlay message while an automatic restart is still to come.
pub const CRASHED_RETRYING_MESSAGE: &str = "Neovim crashed — restarting…";

/// The overlay message once the automatic restart budget is spent and the only
/// remaining route back is the button.
pub const CRASHED_FAILED_MESSAGE: &str = "Neovim crashed — automatic restart failed";

/// The overlay message while the captive Neovim is wedged.
pub const STALLED_MESSAGE: &str = "Neovim stopped responding";

/// The label on the overlay's restart button. Present in BOTH overlay modes: a
/// fallback while a `Dead` core auto-restarts, the primary action for a stalled
/// one.
pub const RESTART_BUTTON_LABEL: &str = "Restart Neovim";

/// The transient banner's message, flashed on the restart SUCCESS path only.
///
/// It is deliberately not a substring of — and does not share the word
/// "restarted" with — any [`overlay_message`] result: the two cage smokes assert
/// on OCR text, and a term that appeared in both would let one satisfy the other.
pub const RESTARTED_BANNER_MESSAGE: &str = "Neovim was restarted";

/// How many AUTOMATIC restart attempts are made per crash before the front-end
/// stops and leaves the button.
///
/// The cap exists solely to stop a retry loop against a permanently failing
/// respawn (`WolfCore::restart_nvim`'s `Err` arm re-marks the core `Dead` and
/// fires another wake, which would otherwise re-enter this path forever). It is
/// NOT a limit on the user: an explicit button press resets the counter.
pub const MAX_AUTO_RESTART_ATTEMPTS: u32 = 1;

/// How long the "Neovim was restarted" banner stays up, in seconds.
pub const BANNER_VISIBLE_SECS: f64 = 4.0;

/// Height (px) reserved for the overlay's message label, excluding the gap below
/// it.
pub const OVERLAY_MESSAGE_HEIGHT: i32 = 56;

/// Gap (px) between the overlay's message and its restart button.
pub const OVERLAY_SPACING: i32 = 24;

/// Width (px) of the overlay's restart button.
pub const RESTART_BUTTON_WIDTH: i32 = 320;

/// Height (px) of the overlay's restart button.
pub const RESTART_BUTTON_HEIGHT: i32 = 64;

/// Maps a liveness reading to the overlay mode, the auto-restart decision and the
/// paint gate.
///
/// TOTAL by construction: a `match` with **no `_` arm**, so a new
/// [`wolf_core::Liveness`] variant is an `E0004` here until it is classified.
///
/// | liveness | `may_auto_restart` | overlay | `auto_restart` | `paint` |
/// | --- | --- | --- | --- | --- |
/// | `Alive` | any | `Hidden` | `false` | `true` |
/// | `NotResponding` | any | `Stalled` | `false` | `false` |
/// | `Dead` | `true` | `Crashed { retrying: true }` | `true` | `false` |
/// | `Dead` | `false` | `Crashed { retrying: false }` | `false` | `false` |
///
/// `Alive` and `NotResponding` are deliberately INSENSITIVE to
/// `may_auto_restart`: a spent attempt budget must never stop the editor
/// painting, and a wedged (but recoverable) nvim must never be force-killed
/// automatically — `restart_nvim` on a stall loses everything not in a swap file,
/// so that decision belongs to the user's button press alone.
#[must_use]
pub fn liveness_ui(liveness: Liveness, may_auto_restart: bool) -> LivenessUi {
    match liveness {
        Liveness::Alive => LivenessUi {
            overlay: OverlayMode::Hidden,
            auto_restart: false,
            paint: true,
        },
        Liveness::NotResponding => LivenessUi {
            overlay: OverlayMode::Stalled,
            auto_restart: false,
            paint: false,
        },
        Liveness::Dead => LivenessUi {
            overlay: OverlayMode::Crashed {
                retrying: may_auto_restart,
            },
            auto_restart: may_auto_restart,
            paint: false,
        },
    }
}

/// The message an overlay mode shows, or `None` when the overlay is hidden.
///
/// TOTAL by construction: a `match` with **no `_` arm** (see the module docs).
#[must_use]
pub fn overlay_message(mode: OverlayMode) -> Option<&'static str> {
    match mode {
        OverlayMode::Hidden => None,
        OverlayMode::Crashed { retrying: true } => Some(CRASHED_RETRYING_MESSAGE),
        OverlayMode::Crashed { retrying: false } => Some(CRASHED_FAILED_MESSAGE),
        OverlayMode::Stalled => Some(STALLED_MESSAGE),
    }
}

/// Classifies the result of a `WolfCore::restart_nvim()` call.
///
/// `respawned` is the `Ok(true)` of that call — an ACTUAL respawn. Anything else
/// (`Ok(false)`: the core was already `Alive`, or a concurrent restart held the
/// core's own CAS; or an `Err`: the respawn/setup chain failed) is a
/// [`RestartOutcome::Refresh`] carrying the mapping of a FRESH liveness read, so
/// the caller re-applies the overlay and does nothing else.
///
/// `Refresh` is built with `may_auto_restart: false`, and that is load-bearing:
/// if a `Refresh` could carry `auto_restart: true`, a persistently failing
/// respawn would retry from INSIDE the restart path, bypassing the attempt cap
/// entirely and spinning forever. Scheduling an automatic attempt is
/// `AppShell::on_liveness_wake`'s job alone; the caller must never act on
/// `Refresh`'s `auto_restart` field.
#[must_use]
pub fn restart_outcome(respawned: bool, fresh: Liveness) -> RestartOutcome {
    if respawned {
        RestartOutcome::Respawned
    } else {
        RestartOutcome::Refresh(liveness_ui(fresh, false))
    }
}

/// Orders the captured Tier-1 file set for reopening, with `active` LAST.
///
/// Every entry of `files` that is not equal to `active` comes first, in input
/// order, then `active` (when `Some`) is appended. Reopening is a sequence of
/// `InputEvent::Open`s on one ordered channel, so the LAST path opened is the one
/// that ends up focused — which is why the active file must be appended rather
/// than merely retained.
///
/// Details that are decisions, not accidents:
///
/// - ALL occurrences of `active` are filtered out of `files`, not just the first.
/// - `active` is appended even when it was NOT a member of `files` (macOS parity
///   — `Project.swift` reopens the active path unconditionally).
/// - Non-active entries are NOT deduplicated (macOS parity; `TabFiles::files` is
///   already unique, and silently collapsing duplicates here would hide a core
///   bug rather than fix one).
/// - `files` empty + `Some(active)` yields `[active]`; `None` active yields
///   `files` verbatim.
#[must_use]
pub fn reopen_order(files: &[PathBuf], active: Option<&Path>) -> Vec<PathBuf> {
    let mut order: Vec<PathBuf> = files
        .iter()
        .filter(|path| Some(path.as_path()) != active)
        .cloned()
        .collect();
    if let Some(active) = active {
        order.push(active.to_path_buf());
    }
    order
}

/// The restart button's rect — `(x, y, w, h)` — RELATIVE TO THE OVERLAY CHILD's
/// own top-left, given the overlay's allocation in pixels.
///
/// This is the SINGLE SOURCE OF TRUTH shared by two consumers that must not
/// drift: [`crate::crash_overlay`] realises the four fixed constants as explicit
/// size requests on the label and the button, and the cage smoke calls this
/// function to derive the pixel it clicks. That sharing is also why the overlay
/// is a hand-built `gtk::Box` rather than an `adw::StatusPage` — a status page's
/// internal spacing is not derivable from anything this crate owns.
///
/// The layout it describes: a fixed vertical block of
/// `OVERLAY_MESSAGE_HEIGHT + OVERLAY_SPACING + RESTART_BUTTON_HEIGHT`, centred
/// both ways in the overlay, with the button occupying the bottom
/// `RESTART_BUTTON_HEIGHT` of it:
///
/// ```text
/// w       = RESTART_BUTTON_WIDTH
/// h       = RESTART_BUTTON_HEIGHT
/// block_h = OVERLAY_MESSAGE_HEIGHT + OVERLAY_SPACING + RESTART_BUTTON_HEIGHT
/// x       = (overlay_w - w) / 2
/// y       = (overlay_h - block_h) / 2 + OVERLAY_MESSAGE_HEIGHT + OVERLAY_SPACING
/// ```
///
/// Degenerate guard, mirroring [`crate::grid_size_for`] /
/// [`crate::cell_at`]'s whole-result-collapse convention: a non-finite or
/// non-positive `overlay_w`/`overlay_h`, or an allocation that cannot contain the
/// fixed block (narrower than `RESTART_BUTTON_WIDTH`, or shorter than `block_h`),
/// collapses the WHOLE result to `(0.0, 0.0, 0.0, 0.0)` rather than returning a
/// partial rect. A caller that derives a click point must therefore check the
/// size before trusting the origin — a collapsed result would otherwise silently
/// retarget a click to the overlay's top-left corner.
///
/// Every `i32 -> f64` widening goes through `f64::from`, never `as`: the values
/// are small named constants, and `f64::from` is total and lossless where a cast
/// would merely happen to be.
#[must_use]
pub fn restart_button_rect(overlay_w: f64, overlay_h: f64) -> (f64, f64, f64, f64) {
    let w = f64::from(RESTART_BUTTON_WIDTH);
    let h = f64::from(RESTART_BUTTON_HEIGHT);
    let block_h = f64::from(OVERLAY_MESSAGE_HEIGHT) + f64::from(OVERLAY_SPACING) + h;

    // A degenerate allocation, or one too small for the fixed block, zeroes the whole result rather
    // than giving a per-axis partial answer.
    if !overlay_w.is_finite()
        || !overlay_h.is_finite()
        || overlay_w <= 0.0
        || overlay_h <= 0.0
        || overlay_w < w
        || overlay_h < block_h
    {
        return (0.0, 0.0, 0.0, 0.0);
    }

    let block_top = (overlay_h - block_h) / 2.0;
    let x = (overlay_w - w) / 2.0;
    let y = block_top + f64::from(OVERLAY_MESSAGE_HEIGHT) + f64::from(OVERLAY_SPACING);
    (x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- The whole input domain: several rules are swept over every input so a failure names the rule.

    /// Every liveness state the core can report.
    const ALL_LIVENESS: [Liveness; 3] = [Liveness::Alive, Liveness::NotResponding, Liveness::Dead];

    /// Both values of `may_auto_restart`: an automatic attempt is still allowed,
    /// or the budget is spent.
    const BOTH_BUDGETS: [bool; 2] = [true, false];

    /// Both values of `respawned`: `WolfCore::restart_nvim()` reported an actual
    /// respawn (`Ok(true)`), or it did not (`Ok(false)` or an `Err`).
    const BOTH_RESPAWN_RESULTS: [bool; 2] = [true, false];

    // === liveness_ui ===

    /// One `liveness_ui` table row.
    struct LivenessCase {
        desc: &'static str,
        liveness: Liveness,
        may_auto_restart: bool,
        expected: LivenessUi,
    }

    /// Pins the three-state mapping, one row per `(liveness, may_auto_restart)`
    /// combination.
    ///
    /// The row count assertion is what makes the table non-vacuous: the domain is
    /// three liveness states times two budget states, so a row that quietly went
    /// missing cannot hide behind the five that remain.
    #[test]
    fn liveness_ui_table() {
        let cases = [
            // --- Alive: the editor is usable, and the budget is irrelevant ----
            LivenessCase {
                desc: "a live core hides the overlay and PAINTS",
                liveness: Liveness::Alive,
                may_auto_restart: true,
                expected: LivenessUi {
                    overlay: OverlayMode::Hidden,
                    auto_restart: false,
                    paint: true,
                },
            },
            LivenessCase {
                desc: "a live core still hides the overlay and PAINTS with the budget spent",
                liveness: Liveness::Alive,
                may_auto_restart: false,
                expected: LivenessUi {
                    overlay: OverlayMode::Hidden,
                    auto_restart: false,
                    paint: true,
                },
            },
            // --- NotResponding: overlay, no paint, and NEVER an auto-restart --
            LivenessCase {
                desc: "a wedged core shows the stall overlay, skips the paint, restarts nothing",
                liveness: Liveness::NotResponding,
                may_auto_restart: true,
                expected: LivenessUi {
                    overlay: OverlayMode::Stalled,
                    auto_restart: false,
                    paint: false,
                },
            },
            LivenessCase {
                desc: "a wedged core is unchanged by a spent budget",
                liveness: Liveness::NotResponding,
                may_auto_restart: false,
                expected: LivenessUi {
                    overlay: OverlayMode::Stalled,
                    auto_restart: false,
                    paint: false,
                },
            },
            // --- Dead: the only state that ever asks for an automatic restart -
            LivenessCase {
                desc: "a dead core with attempts left retries, and says so in the overlay",
                liveness: Liveness::Dead,
                may_auto_restart: true,
                expected: LivenessUi {
                    overlay: OverlayMode::Crashed { retrying: true },
                    auto_restart: true,
                    paint: false,
                },
            },
            LivenessCase {
                desc: "a dead core with the budget spent stops retrying and leaves the button",
                liveness: Liveness::Dead,
                may_auto_restart: false,
                expected: LivenessUi {
                    overlay: OverlayMode::Crashed { retrying: false },
                    auto_restart: false,
                    paint: false,
                },
            },
        ];

        assert_eq!(
            cases.len(),
            ALL_LIVENESS.len() * BOTH_BUDGETS.len(),
            "the liveness_ui table must carry exactly one row per (liveness, may_auto_restart) \
             combination: three liveness states times two budget states is six rows"
        );

        for case in &cases {
            let got = liveness_ui(case.liveness, case.may_auto_restart);
            assert_eq!(
                got, case.expected,
                "liveness_ui({:?}, may_auto_restart = {}) must be {:?} — {}",
                case.liveness, case.may_auto_restart, case.expected, case.desc
            );
        }

        // The two non-dead states ignore the attempt budget: an `Alive` that stopped painting at the cap
        // would freeze a healthy editor, and nothing else would notice.
        assert_eq!(
            liveness_ui(Liveness::Alive, true),
            liveness_ui(Liveness::Alive, false),
            "an ALIVE core must be completely insensitive to the automatic-restart budget: a \
             spent budget must never stop the editor painting"
        );
        assert_eq!(
            liveness_ui(Liveness::NotResponding, true),
            liveness_ui(Liveness::NotResponding, false),
            "a NOT-RESPONDING core must be completely insensitive to the automatic-restart \
             budget: the budget only governs the DEAD path"
        );
    }

    /// States the auto-restart rule over the whole domain, so a failure names the
    /// rule rather than a table row: exactly one of the six inputs — a dead core
    /// with attempts left — may ask for an automatic restart.
    #[test]
    fn only_dead_auto_restarts() {
        let mut requesting: Vec<(Liveness, bool)> = Vec::new();
        for liveness in ALL_LIVENESS {
            for may_auto_restart in BOTH_BUDGETS {
                if liveness_ui(liveness, may_auto_restart).auto_restart {
                    requesting.push((liveness, may_auto_restart));
                }
            }
        }

        let expected = [(Liveness::Dead, true)];
        assert_eq!(
            requesting.as_slice(),
            expected.as_slice(),
            "exactly ONE of the six (liveness, may_auto_restart) inputs may request an automatic \
             restart — a DEAD core with attempts left. Anything else here either restarts a core \
             that is fine (or merely wedged), or refuses to restart one that has crashed."
        );
    }

    /// Pinned on its own because it is the one place a "just restart it"
    /// simplification would silently destroy a user's work.
    #[test]
    fn not_responding_never_auto_restarts() {
        for may_auto_restart in BOTH_BUDGETS {
            let ui = liveness_ui(Liveness::NotResponding, may_auto_restart);
            assert!(
                !ui.auto_restart,
                "a WEDGED (NotResponding) Neovim must never be restarted automatically, whatever \
                 the attempt budget says (may_auto_restart = {may_auto_restart}): restart_nvim \
                 force-kills the process, so everything not already in a swap file is lost. That \
                 decision belongs to the user's button press alone."
            );
            assert_eq!(
                ui.overlay,
                OverlayMode::Stalled,
                "a wedged Neovim must show the STALL overlay (may_auto_restart = \
                 {may_auto_restart}), never the crash overlay: it has not died, and the core can \
                 still clear the state by itself when the RPC finally returns"
            );
            assert!(
                !ui.paint,
                "a wedged Neovim must not be painted (may_auto_restart = {may_auto_restart}): the \
                 snapshot is frozen at the last frame and the opaque overlay hides it anyway"
            );
        }
    }

    // === overlay_message ===

    /// One `overlay_message` table row.
    struct MessageCase {
        desc: &'static str,
        mode: OverlayMode,
        expected: Option<&'static str>,
    }

    /// The word the crash-overlay cage smoke asserts on, lowercased.
    const OVERLAY_OCR_TERM: &str = "crashed";

    /// The word the restart-banner cage smoke asserts on, lowercased.
    const BANNER_OCR_TERM: &str = "restarted";

    /// The word the STALL cage smoke
    /// (`stalled_nvim_raises_the_stall_overlay_under_cage`) asserts on, lowercased.
    ///
    /// That smoke wedges the captive Neovim with `SIGSTOP`, fires Ctrl+Shift+E so
    /// the buffer picker's blocking `list_buffers` elapses its timeout budget, and
    /// reads this term off the raised overlay. Nothing else on that page may
    /// produce it, which is what the block below pins in both directions.
    const STALL_OCR_TERM: &str = "responding";

    /// Pins the message each overlay mode carries, and — just as importantly —
    /// keeps the two cage smokes' OCR terms from cross-satisfying each other.
    #[test]
    fn overlay_message_table() {
        let cases = [
            MessageCase {
                desc: "a hidden overlay carries no message at all",
                mode: OverlayMode::Hidden,
                expected: None,
            },
            MessageCase {
                desc: "a crash with an automatic restart still to come",
                mode: OverlayMode::Crashed { retrying: true },
                expected: Some(CRASHED_RETRYING_MESSAGE),
            },
            MessageCase {
                desc: "a crash whose automatic restart budget is spent",
                mode: OverlayMode::Crashed { retrying: false },
                expected: Some(CRASHED_FAILED_MESSAGE),
            },
            MessageCase {
                desc: "a wedged Neovim",
                mode: OverlayMode::Stalled,
                expected: Some(STALLED_MESSAGE),
            },
        ];

        for case in &cases {
            let got = overlay_message(case.mode);
            assert_eq!(
                got, case.expected,
                "overlay_message({:?}) must be {:?} — {}",
                case.mode, case.expected, case.desc
            );
        }

        // The two crash messages tell "it is coming back" from "it is not"; identical strings would hide
        // the retrying flag.
        assert_ne!(
            overlay_message(OverlayMode::Crashed { retrying: true }),
            overlay_message(OverlayMode::Crashed { retrying: false }),
            "the two Crashed modes must carry DISTINCT messages: they are the only way a user can \
             tell an automatic restart that is still coming from one that will not come"
        );

        // --- OCR cross-satisfaction guard: smoke A reads the banner term on a recovered window, smoke B
        // the overlay term on a crash overlay; neither term may come from the other's screen.
        for retrying in [true, false] {
            let message = overlay_message(OverlayMode::Crashed { retrying })
                .expect("a Crashed overlay always carries a message")
                .to_lowercase();
            assert!(
                message.contains(OVERLAY_OCR_TERM),
                "the Crashed{{ retrying: {retrying} }} message ({message:?}) must contain \
                 {OVERLAY_OCR_TERM:?} once lowercased — that is the term the crash-overlay cage \
                 smoke reads off the screen"
            );
            assert!(
                !message.contains(BANNER_OCR_TERM),
                "the Crashed{{ retrying: {retrying} }} message ({message:?}) must NOT contain \
                 {BANNER_OCR_TERM:?} once lowercased: the restart BANNER is the only thing \
                 allowed to produce that term, or the banner smoke would be satisfied by a crash \
                 overlay it should never have seen"
            );
            assert!(
                !message.contains(STALL_OCR_TERM),
                "the Crashed{{ retrying: {retrying} }} message ({message:?}) must NOT contain \
                 {STALL_OCR_TERM:?} once lowercased: the STALL overlay is the only thing allowed \
                 to produce that term. A crash and a stall are different states with different \
                 routes back — one auto-restarts, the other waits for the user's button — so a \
                 shared word would let the stall smoke pass against a CRASHED core, i.e. against \
                 a Neovim that died rather than wedged"
            );
        }

        assert!(
            !STALLED_MESSAGE.to_lowercase().contains(BANNER_OCR_TERM),
            "the stall message ({STALLED_MESSAGE:?}) must not contain {BANNER_OCR_TERM:?} either, \
             for the same reason the crash messages must not"
        );

        // The positive half of the stall term, without which the "must not contain" assertions are idle:
        // the stall smoke reads this word off the raised overlay.
        assert!(
            STALLED_MESSAGE.to_lowercase().contains(STALL_OCR_TERM),
            "the stall message ({STALLED_MESSAGE:?}) must contain {STALL_OCR_TERM:?} once \
             lowercased — it is the only text the stall overlay draws, and the term \
             `stalled_nvim_raises_the_stall_overlay_under_cage` reads off the capture. Rewording \
             this message to drop the word (\"Neovim is wedged\", say) silently DISARMS that cage \
             smoke: the overlay would still be raised correctly and the smoke would still go red, \
             with nothing naming the wording as the cause. Change the smoke's STALL_OCR_PRESENT \
             const together with this string, or leave both alone."
        );
        assert!(
            !RESTARTED_BANNER_MESSAGE
                .to_lowercase()
                .contains(STALL_OCR_TERM),
            "the banner message ({RESTARTED_BANNER_MESSAGE:?}) must not contain \
             {STALL_OCR_TERM:?}: the banner flashes on the restart SUCCESS path, so a shared word \
             would let the stall smoke's must-CONTAIN assertion be satisfied by a window that had \
             just RECOVERED — the opposite of the state it is asserting on"
        );

        // The button sits INSIDE the crash overlay, so its label is on screen at
        // exactly the moment smoke B captures — it must not contribute either term.
        let button = RESTART_BUTTON_LABEL.to_lowercase();
        assert!(
            !button.contains(BANNER_OCR_TERM),
            "the restart button label ({RESTART_BUTTON_LABEL:?}) must not contain \
             {BANNER_OCR_TERM:?}: the button is visible in the crash overlay, so a label \
             containing it would let the overlay satisfy the banner assertion"
        );
        assert!(
            !button.contains(OVERLAY_OCR_TERM),
            "the restart button label ({RESTART_BUTTON_LABEL:?}) must not contain \
             {OVERLAY_OCR_TERM:?}: the overlay term must come from the MESSAGE, so that hiding \
             the message alone is enough to turn the smoke red"
        );
        assert!(
            !button.contains(STALL_OCR_TERM),
            "the restart button label ({RESTART_BUTTON_LABEL:?}) must not contain \
             {STALL_OCR_TERM:?}. The button sits INSIDE the stall overlay, so its label is on \
             screen at exactly the moment `stalled_nvim_raises_the_stall_overlay_under_cage` \
             captures: a label carrying the term would satisfy that smoke's must-CONTAIN \
             assertion even with the stall MESSAGE never drawn"
        );

        // The positive half: the banner really does carry the term the smoke reads,
        // without which every "must not contain" assertion above would be idle.
        assert!(
            RESTARTED_BANNER_MESSAGE
                .to_lowercase()
                .contains(BANNER_OCR_TERM),
            "the banner message ({RESTARTED_BANNER_MESSAGE:?}) must contain {BANNER_OCR_TERM:?} \
             once lowercased — it is the only user-visible confirmation on the restart success \
             path, and the term the banner cage smoke reads"
        );
        assert!(
            !RESTARTED_BANNER_MESSAGE
                .to_lowercase()
                .contains(OVERLAY_OCR_TERM),
            "the banner message ({RESTARTED_BANNER_MESSAGE:?}) must not contain \
             {OVERLAY_OCR_TERM:?}: the banner flashes on the SUCCESS path, so it must not be able \
             to satisfy an assertion about a crash overlay"
        );
    }

    // === restart_outcome ===

    /// One `restart_outcome` table row.
    struct OutcomeCase {
        desc: &'static str,
        respawned: bool,
        fresh: Liveness,
        expected: RestartOutcome,
    }

    /// Pins how the result of `WolfCore::restart_nvim()` is classified, given that
    /// its `Ok(false)` is a no-op rather than a failure.
    #[test]
    fn restart_outcome_table() {
        let cases = [
            // --- An ACTUAL respawn: nothing else is consulted -----------------
            OutcomeCase {
                desc: "a respawn is a respawn even before the fresh nvim reports Alive",
                respawned: true,
                fresh: Liveness::Alive,
                expected: RestartOutcome::Respawned,
            },
            OutcomeCase {
                desc: "a respawn is not downgraded by a fresh NotResponding reading",
                respawned: true,
                fresh: Liveness::NotResponding,
                expected: RestartOutcome::Respawned,
            },
            OutcomeCase {
                desc: "a respawn is not downgraded by a fresh Dead reading either",
                respawned: true,
                fresh: Liveness::Dead,
                expected: RestartOutcome::Respawned,
            },
            // --- No respawn: re-apply the overlay from a FRESH reading ---------
            OutcomeCase {
                desc: "a redundant restart against a live core just re-shows the live UI",
                respawned: false,
                fresh: Liveness::Alive,
                expected: RestartOutcome::Refresh(LivenessUi {
                    overlay: OverlayMode::Hidden,
                    auto_restart: false,
                    paint: true,
                }),
            },
            OutcomeCase {
                desc: "no respawn against a wedged core keeps the stall overlay up",
                respawned: false,
                fresh: Liveness::NotResponding,
                expected: RestartOutcome::Refresh(LivenessUi {
                    overlay: OverlayMode::Stalled,
                    auto_restart: false,
                    paint: false,
                }),
            },
            OutcomeCase {
                desc: "a FAILED respawn keeps the crash overlay up, with retrying already false",
                respawned: false,
                fresh: Liveness::Dead,
                expected: RestartOutcome::Refresh(LivenessUi {
                    overlay: OverlayMode::Crashed { retrying: false },
                    auto_restart: false,
                    paint: false,
                }),
            },
        ];

        assert_eq!(
            cases.len(),
            BOTH_RESPAWN_RESULTS.len() * ALL_LIVENESS.len(),
            "the restart_outcome table must carry one row per (respawned, fresh liveness) \
             combination: two respawn results times three liveness states is six rows"
        );

        for case in &cases {
            let got = restart_outcome(case.respawned, case.fresh);
            assert_eq!(
                got, case.expected,
                "restart_outcome(respawned = {}, fresh = {:?}) must be {:?} — {}",
                case.respawned, case.fresh, case.expected, case.desc
            );
        }
    }

    /// The infinite-respawn-loop guard, stated over every fresh liveness reading.
    ///
    /// If a `Refresh` could ever carry `auto_restart: true`, a persistently failing
    /// respawn would retry from INSIDE the restart path, bypassing the attempt cap
    /// entirely and spinning for as long as the window is open.
    #[test]
    fn refresh_never_requests_an_auto_restart() {
        for fresh in ALL_LIVENESS {
            match restart_outcome(false, fresh) {
                RestartOutcome::Refresh(ui) => assert!(
                    !ui.auto_restart,
                    "restart_outcome(false, {fresh:?}) returned a Refresh carrying \
                     auto_restart = true. A Refresh must NEVER request an automatic restart: \
                     scheduling one is on_liveness_wake's job alone, and a Refresh that asked for \
                     one would retry a failing respawn forever, ignoring \
                     MAX_AUTO_RESTART_ATTEMPTS."
                ),
                RestartOutcome::Respawned => panic!(
                    "restart_outcome(false, {fresh:?}) must be a Refresh: `respawned == false` \
                     means no respawn happened, so the caller must not reset the pane view, \
                     reopen files, or flash the banner"
                ),
            }
        }

        for fresh in ALL_LIVENESS {
            assert_eq!(
                restart_outcome(true, fresh),
                RestartOutcome::Respawned,
                "restart_outcome(true, {fresh:?}) must be Respawned and carry no LivenessUi at \
                 all — an actual respawn is decided by the core's Ok(true), never by a liveness \
                 reading that may not have caught up yet"
            );
        }
    }

    // === reopen_order ===

    /// A file in the captured Tier-1 set.
    const ALPHA: &str = "/w/alpha.md";
    /// A second file in the captured Tier-1 set.
    const BRAVO: &str = "/w/bravo.md";
    /// A third file in the captured Tier-1 set.
    const CHARLIE: &str = "/w/charlie.md";
    /// The recovered unnamed buffer `wolf-core`'s restart tests observe in a real
    /// captured set, so at least one row is the shape production actually sees.
    const UNTITLED: &str = "/w/Untitled.md";

    /// One `reopen_order` table row.
    struct ReopenCase {
        desc: &'static str,
        files: &'static [&'static str],
        active: Option<&'static str>,
        expected: &'static [&'static str],
    }

    /// The reopen-order scenarios, as data.
    ///
    /// A `const` item (rather than a local `let` array) because two tests walk the
    /// same rows — the table pins each expected ORDER, and
    /// `active_is_always_last` states the one rule that must hold across all of
    /// them. Same shape as `mouse::tests::ACC_CASES`.
    const REOPEN_CASES: &[ReopenCase] = &[
        ReopenCase {
            desc: "an active file earlier in the set is moved to the END so it lands focused",
            files: &[ALPHA, UNTITLED],
            active: Some(ALPHA),
            expected: &[UNTITLED, ALPHA],
        },
        ReopenCase {
            desc: "the real crash-recovery set: [Untitled.md, bravo] with bravo active",
            files: &[UNTITLED, BRAVO],
            active: Some(BRAVO),
            expected: &[UNTITLED, BRAVO],
        },
        ReopenCase {
            desc: "an active file that is NOT a member of the set is appended anyway",
            files: &[ALPHA, BRAVO],
            active: Some(CHARLIE),
            expected: &[ALPHA, BRAVO, CHARLIE],
        },
        ReopenCase {
            desc: "no active file: the set is reopened verbatim",
            files: &[ALPHA, BRAVO, CHARLIE],
            active: None,
            expected: &[ALPHA, BRAVO, CHARLIE],
        },
        ReopenCase {
            desc: "an empty set with an active file yields just that file",
            files: &[],
            active: Some(ALPHA),
            expected: &[ALPHA],
        },
        ReopenCase {
            desc: "an empty set with no active file yields nothing to reopen",
            files: &[],
            active: None,
            expected: &[],
        },
        ReopenCase {
            desc: "EVERY occurrence of the active path is filtered, not merely the first",
            files: &[ALPHA, BRAVO, ALPHA, CHARLIE, ALPHA],
            active: Some(ALPHA),
            expected: &[BRAVO, CHARLIE, ALPHA],
        },
        ReopenCase {
            desc: "non-active duplicates are KEPT: not deduplicating is a decision",
            files: &[BRAVO, ALPHA, BRAVO, CHARLIE],
            active: Some(ALPHA),
            expected: &[BRAVO, BRAVO, CHARLIE, ALPHA],
        },
        ReopenCase {
            desc: "the relative order of the non-active entries is preserved",
            files: &[CHARLIE, BRAVO, ALPHA, UNTITLED],
            active: Some(BRAVO),
            expected: &[CHARLIE, ALPHA, UNTITLED, BRAVO],
        },
        ReopenCase {
            desc: "the active file is the only entry",
            files: &[ALPHA],
            active: Some(ALPHA),
            expected: &[ALPHA],
        },
        ReopenCase {
            desc: "no active file: even a duplicate is reopened verbatim",
            files: &[ALPHA, ALPHA],
            active: None,
            expected: &[ALPHA, ALPHA],
        },
    ];

    /// Materializes a row's `files` into the owned form `reopen_order` takes.
    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().copied().map(PathBuf::from).collect()
    }

    #[test]
    fn reopen_order_table() {
        for case in REOPEN_CASES {
            let files = paths(case.files);
            let active = case.active.map(Path::new);
            let got = reopen_order(&files, active);
            assert_eq!(
                got,
                paths(case.expected),
                "reopen_order({:?}, {:?}) must reopen {:?} — {}",
                case.files,
                case.active,
                case.expected,
                case.desc
            );
        }
    }

    /// The one rule that must hold over every row, stated on its own so a failure
    /// names the rule rather than a row index.
    ///
    /// Reopening is a sequence of `InputEvent::Open`s on one ordered channel, so
    /// the LAST path opened is the one that ends up focused. This is exactly the
    /// property the crash-recovery cage smoke discriminates end to end: reopening
    /// the active file first instead of last leaves the recovered `Untitled.md`
    /// focused, and the smoke's OCR of the fixture's heading goes missing.
    #[test]
    fn active_is_always_last() {
        for case in REOPEN_CASES {
            let files = paths(case.files);
            let got = reopen_order(&files, case.active.map(Path::new));

            let Some(active) = case.active.map(Path::new) else {
                assert_eq!(
                    got, files,
                    "reopen_order with NO active file must return the captured set verbatim \
                     — {}",
                    case.desc
                );
                continue;
            };

            assert_eq!(
                got.last().map(PathBuf::as_path),
                Some(active),
                "reopen_order({:?}, {:?}) put {:?} last; the ACTIVE path must always be reopened \
                 LAST so it is the buffer that ends up focused — {}",
                case.files,
                case.active,
                got.last(),
                case.desc
            );

            let hits = got.iter().filter(|path| path.as_path() == active).count();
            assert_eq!(
                hits, 1,
                "reopen_order({:?}, {:?}) reopened the active path {hits} times; it must appear \
                 EXACTLY once — every occurrence is filtered out of the captured set and then it \
                 is appended a single time — {}",
                case.files, case.active, case.desc
            );
        }
    }

    // === restart_button_rect ===

    /// The button's fixed width in pixels, per the contract. Written as a literal
    /// rather than derived from `RESTART_BUTTON_WIDTH` so that the table is an
    /// independent statement of the contract; `named_constants_match_contract`
    /// pins the constant itself.
    const BUTTON_W: f64 = 320.0;

    /// The button's fixed height in pixels, per the contract.
    const BUTTON_H: f64 = 64.0;

    /// The fixed vertical block the overlay centres: the message height (56), the
    /// gap (24) and the button height (64).
    const BLOCK_H: f64 = 144.0;

    /// The whole-result collapse a degenerate or too-small allocation yields.
    ///
    /// It is an unambiguous sentinel rather than a coincidence: a real result has
    /// `y = (overlay_h - 144)/2 + 80`, which is only `0.0` for an `overlay_h` of
    /// `-16`, and a negative allocation collapses anyway.
    const COLLAPSED: (f64, f64, f64, f64) = (0.0, 0.0, 0.0, 0.0);

    /// One `restart_button_rect` table row.
    struct RectCase {
        desc: &'static str,
        overlay_w: f64,
        overlay_h: f64,
        expected: (f64, f64, f64, f64),
    }

    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "every expected component is a hand-derived halving or sum of the contract's \
                  small integer constants — all exactly representable in binary f64 — and the \
                  degenerate rows expect literal zeroes, so an epsilon comparison would weaken \
                  the assertion without making it more correct (the mouse::scroll_threshold_table \
                  precedent)"
    )]
    fn restart_button_rect_table() {
        let cases = [
            // --- Real allocations ---------------------------------------------
            RectCase {
                // x = (1022.1 - 320)/2 = 351.05; y = (668 - 144)/2 + 80 = 342.
                desc: "a typical cage-sized allocation",
                overlay_w: 1022.1,
                overlay_h: 668.0,
                expected: (351.05, 342.0, BUTTON_W, BUTTON_H),
            },
            RectCase {
                // x = (1029 - 320)/2 = 354.5; y = (674 - 144)/2 + 80 = 345: an allocation measured under cage,
                // where the rendered button really occupied this rect.
                desc: "the measured cage allocation, whose rendered button was inspected",
                overlay_w: 1029.0,
                overlay_h: 674.0,
                expected: (354.5, 345.0, BUTTON_W, BUTTON_H),
            },
            RectCase {
                // x = (800 - 320)/2 = 240; y = (800 - 144)/2 + 80 = 408.
                desc: "a square allocation, so width math and height math cannot be confused",
                overlay_w: 800.0,
                overlay_h: 800.0,
                expected: (240.0, 408.0, BUTTON_W, BUTTON_H),
            },
            // --- Exactly on the two lower bounds, which are INCLUSIVE ----------
            RectCase {
                // y = (144 - 144)/2 + 80 = 80, and 80 + 64 == 144 == overlay_h.
                desc: "an allocation exactly the fixed block's height still fits",
                overlay_w: 640.0,
                overlay_h: BLOCK_H,
                expected: (160.0, 80.0, BUTTON_W, BUTTON_H),
            },
            RectCase {
                // x = (320 - 320)/2 = 0, and 0 + 320 == 320 == overlay_w.
                desc: "an allocation exactly the button's width still fits, flush at x = 0",
                overlay_w: BUTTON_W,
                overlay_h: 400.0,
                expected: (0.0, 208.0, BUTTON_W, BUTTON_H),
            },
            // --- Too small in one axis: the WHOLE result collapses -------------
            RectCase {
                desc: "one pixel shorter than the fixed block collapses the whole result",
                overlay_w: 640.0,
                overlay_h: 143.0,
                expected: COLLAPSED,
            },
            RectCase {
                desc: "one pixel narrower than the button collapses the whole result",
                overlay_w: 319.0,
                overlay_h: 668.0,
                expected: COLLAPSED,
            },
            // --- Degenerate in each axis, independently ------------------------
            RectCase {
                desc: "a zero width collapses",
                overlay_w: 0.0,
                overlay_h: 668.0,
                expected: COLLAPSED,
            },
            RectCase {
                desc: "a zero height collapses",
                overlay_w: 1022.1,
                overlay_h: 0.0,
                expected: COLLAPSED,
            },
            RectCase {
                desc: "a negative width collapses",
                overlay_w: -1022.1,
                overlay_h: 668.0,
                expected: COLLAPSED,
            },
            RectCase {
                desc: "a negative height collapses",
                overlay_w: 1022.1,
                overlay_h: -668.0,
                expected: COLLAPSED,
            },
            RectCase {
                desc: "a NaN width collapses",
                overlay_w: f64::NAN,
                overlay_h: 668.0,
                expected: COLLAPSED,
            },
            RectCase {
                desc: "a NaN height collapses",
                overlay_w: 1022.1,
                overlay_h: f64::NAN,
                expected: COLLAPSED,
            },
            RectCase {
                desc: "an infinite width collapses",
                overlay_w: f64::INFINITY,
                overlay_h: 668.0,
                expected: COLLAPSED,
            },
            RectCase {
                desc: "an infinite height collapses",
                overlay_w: 1022.1,
                overlay_h: f64::INFINITY,
                expected: COLLAPSED,
            },
            RectCase {
                desc: "a negatively-infinite width collapses",
                overlay_w: f64::NEG_INFINITY,
                overlay_h: 668.0,
                expected: COLLAPSED,
            },
            RectCase {
                desc: "a negatively-infinite height collapses",
                overlay_w: 1022.1,
                overlay_h: f64::NEG_INFINITY,
                expected: COLLAPSED,
            },
        ];

        for case in &cases {
            let got = restart_button_rect(case.overlay_w, case.overlay_h);
            assert_eq!(
                got, case.expected,
                "restart_button_rect({}, {}) must be {:?} — {}",
                case.overlay_w, case.overlay_h, case.expected, case.desc
            );
        }
    }

    /// One containment-property row: an allocation and whether the contract says
    /// it collapses.
    struct FitCase {
        desc: &'static str,
        overlay_w: f64,
        overlay_h: f64,
        collapses: bool,
    }

    /// The composition guarantee, made executable: a non-degenerate button rect is
    /// always wholly INSIDE the overlay it was derived from.
    ///
    /// Mirrors `geometry::tests::cell_at_never_exceeds_grid_size_for`. It is also
    /// what discriminates the width guard: without it, an overlay narrower than
    /// the button yields a negative `x`, and the cage smoke would click outside
    /// the window rather than on the button.
    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "the collapse rows compare against literal zeroes returned unmodified, so an \
                  epsilon would weaken the assertion; the containment half uses only >= / <=, \
                  over sums that are exactly representable"
    )]
    fn restart_button_rect_is_inside_the_overlay() {
        let cases = [
            FitCase {
                desc: "a typical cage-sized allocation",
                overlay_w: 1022.1,
                overlay_h: 668.0,
                collapses: false,
            },
            FitCase {
                desc: "the measured cage allocation",
                overlay_w: 1029.0,
                overlay_h: 674.0,
                collapses: false,
            },
            FitCase {
                desc: "the whole cage output, had the sidebar been collapsed",
                overlay_w: 1280.0,
                overlay_h: 720.0,
                collapses: false,
            },
            FitCase {
                desc: "a square allocation",
                overlay_w: 800.0,
                overlay_h: 800.0,
                collapses: false,
            },
            FitCase {
                desc: "a very large allocation",
                overlay_w: 4096.0,
                overlay_h: 2160.0,
                collapses: false,
            },
            FitCase {
                desc: "exactly on both lower bounds: the button width and the block height",
                overlay_w: BUTTON_W,
                overlay_h: BLOCK_H,
                collapses: false,
            },
            FitCase {
                desc: "one pixel over both lower bounds, so x and y are both fractional",
                overlay_w: 321.0,
                overlay_h: 145.0,
                collapses: false,
            },
            FitCase {
                desc: "narrower than the button: collapses rather than yielding a negative x",
                overlay_w: 319.0,
                overlay_h: 668.0,
                collapses: true,
            },
            FitCase {
                desc: "far narrower than the button",
                overlay_w: 100.0,
                overlay_h: 668.0,
                collapses: true,
            },
            FitCase {
                desc: "shorter than the fixed block: collapses rather than overflowing the bottom",
                overlay_w: 640.0,
                overlay_h: 143.0,
                collapses: true,
            },
            FitCase {
                desc: "a zero allocation",
                overlay_w: 0.0,
                overlay_h: 0.0,
                collapses: true,
            },
            FitCase {
                desc: "a negative allocation",
                overlay_w: -5.0,
                overlay_h: 668.0,
                collapses: true,
            },
            FitCase {
                desc: "a NaN allocation",
                overlay_w: f64::NAN,
                overlay_h: f64::NAN,
                collapses: true,
            },
            FitCase {
                desc: "an infinite allocation",
                overlay_w: f64::INFINITY,
                overlay_h: f64::INFINITY,
                collapses: true,
            },
        ];

        for case in &cases {
            let got = restart_button_rect(case.overlay_w, case.overlay_h);
            let (x, y, w, h) = got;

            if case.collapses {
                assert_eq!(
                    got, COLLAPSED,
                    "restart_button_rect({}, {}) must collapse the WHOLE result to (0, 0, 0, 0) \
                     rather than return a partial rect — {}",
                    case.overlay_w, case.overlay_h, case.desc
                );
                continue;
            }

            assert_eq!(
                (w, h),
                (BUTTON_W, BUTTON_H),
                "restart_button_rect({}, {}) must report the button's fixed size — a collapsed \
                 result here would silently retarget a derived click to the overlay's top-left \
                 corner — {}",
                case.overlay_w,
                case.overlay_h,
                case.desc
            );
            assert!(
                x >= 0.0,
                "restart_button_rect({}, {}) put the button at x = {x}, left of the overlay's own \
                 origin — {}",
                case.overlay_w,
                case.overlay_h,
                case.desc
            );
            assert!(
                y >= 0.0,
                "restart_button_rect({}, {}) put the button at y = {y}, above the overlay's own \
                 origin — {}",
                case.overlay_w,
                case.overlay_h,
                case.desc
            );
            assert!(
                x + w <= case.overlay_w,
                "restart_button_rect({}, {}) put the button's right edge at {}, past the \
                 overlay's width — {}",
                case.overlay_w,
                case.overlay_h,
                x + w,
                case.desc
            );
            assert!(
                y + h <= case.overlay_h,
                "restart_button_rect({}, {}) put the button's bottom edge at {}, past the \
                 overlay's height — {}",
                case.overlay_w,
                case.overlay_h,
                y + h,
                case.desc
            );
        }
    }

    // === The named constants ===

    /// Pins the documented named constants, mirroring
    /// `cursor::tests::named_constants_match_contract`.
    ///
    /// The geometry four are what the widget builder realises as size requests and
    /// what `restart_button_rect` derives from, so a smoke that clicks a derived
    /// pixel cannot be silently retargeted by a drift here. The other two are
    /// product decisions: one automatic attempt per crash, and a four-second
    /// banner.
    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "BANNER_VISIBLE_SECS is pinned to 4.0, which is exactly representable in binary \
                  f64, and the whole point of the assertion is that the constant is EXACTLY that \
                  value — an epsilon would let a drift through"
    )]
    fn named_constants_match_contract() {
        assert_eq!(
            OVERLAY_MESSAGE_HEIGHT, 56,
            "OVERLAY_MESSAGE_HEIGHT must be 56 px: it is the first term of the fixed block \
             restart_button_rect centres, and the label's size request in the overlay builder"
        );
        assert_eq!(
            OVERLAY_SPACING, 24,
            "OVERLAY_SPACING must be 24 px: it is the gap between the overlay's message and its \
             button, realised as the label's bottom margin (never as gtk::Box spacing, which \
             would apply between every pair of children and break the derivation)"
        );
        assert_eq!(
            RESTART_BUTTON_WIDTH, 320,
            "RESTART_BUTTON_WIDTH must be 320 px: the button's size request and the width \
             restart_button_rect reports"
        );
        assert_eq!(
            RESTART_BUTTON_HEIGHT, 64,
            "RESTART_BUTTON_HEIGHT must be 64 px: the button's size request, and the vertical \
             slack that absorbs GTK's odd-leftover rounding when a cage smoke clicks the derived \
             centre"
        );
        assert_eq!(
            MAX_AUTO_RESTART_ATTEMPTS, 1,
            "MAX_AUTO_RESTART_ATTEMPTS must be 1: exactly one AUTOMATIC attempt per crash, which \
             is what stops a permanently failing respawn from spinning forever. It is not a limit \
             on the user — an explicit button press resets the counter."
        );
        assert_eq!(
            BANNER_VISIBLE_SECS, 4.0,
            "BANNER_VISIBLE_SECS must be 4.0: the restart banner is the only user-visible \
             confirmation on the success path. It is a PRODUCT constant — if a cage smoke ever \
             races it, shorten the gap before the screenshot rather than lengthening this."
        );
    }
}
