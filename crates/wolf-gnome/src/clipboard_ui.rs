//! The pure, GTK-free rulebook for COPY and PASTE through the system clipboard:
//! whether each chord may act at all, and what — if anything — crosses the
//! clipboard boundary in each direction.
//!
//! The clipboard counterpart of [`crate::nav_ui`] and [`crate::buffer_picker`].
//! Data in, data out — no widget type, no `gdk::Clipboard`, no `WolfCore` — which
//! is what lets the whole rulebook be unit-tested headlessly. The widget work of
//! carrying the answers out lives in [`crate::shell`], which owns the
//! `gdk::Clipboard` handle and the asynchronous read.
//!
//! ## The blocking call is COPY's alone
//!
//! `WolfCore::copy_visual_selection` yanks over a bounded blocking RPC, so
//! [`copy_decision`] is liveness-gated exactly the way
//! [`crate::buffer_picker::open_decision`] and [`crate::nav_ui::nav_press`] are.
//! The inventory of this crate's blocking calls and the reasoning behind the gate
//! live in `crates/wolf-gnome/DECISIONS.md`, "Crash and stall recovery", and are
//! deliberately not restated here.
//!
//! ## PASTE's refusals are NOT about blocking, and must not be simplified away
//!
//! Nothing on the paste path can block: `gdk::Clipboard::read_text_async` resolves
//! on a later main-loop turn, and `WolfCore::paste` is a send on the ordered input
//! channel. A reader who knows only the paragraph above will therefore read
//! [`paste_decision`]'s refusals as redundant with [`copy_decision`]'s and delete
//! them. They are not redundant, and the reason is different in kind: a paste
//! queued against a core that is `Dead`, `NotResponding` or already taken out is
//! not dropped, it is DEFERRED — an opaque crash or stall overlay is up, the user
//! cannot see what they are pasting into, and the text would be delivered to a
//! RESTARTED Neovim minutes later, at whatever cursor position that one happens to
//! have. Refusing loudly is the only outcome the user can act on.
//!
//! ## Writing nothing is not the same as clearing
//!
//! [`clipboard_write`] answering `None` means **write nothing at all**. It does not
//! mean clear the clipboard, which is the call a reader reaches for when they see
//! an "empty" answer: clearing would destroy whatever the user copied from another
//! application, in response to a chord that failed to yank anything. The rule
//! restates `WolfCore::copy_visual_selection`'s own empty-selection guard on
//! purpose — it is defence in depth stated at the point where the write happens,
//! not an accident to be deduplicated.
//!
//! [`paste_payload`] is the same shape in the other direction: an empty read
//! queues no [`wolf_core::InputEvent`] at all, rather than pasting an empty string
//! and putting an undo entry in the user's buffer for nothing. It takes
//! `Option<&str>` rather than the `Result<Option<glib::GString>, glib::Error>` the
//! GDK callback is handed, so the rule stays constructible in a headless test;
//! [`crate::shell`] flattens the `Result` — printing the `Err`, never swallowing
//! it — before calling in.
//!
//! ## Totality is the contract
//!
//! Both decisions match `Option<Liveness>` with **no `_` arm**, so appending a
//! variant to [`wolf_core::Liveness`] is an `E0004` compile error here until it is
//! classified — the same guard [`crate::nav_ui::nav_press`] and
//! [`crate::liveness_ui`] rely on. `Option<Liveness>` (not `Liveness`) is the input
//! type precisely so the ABSENT core, which the window's close handler leaves
//! behind, is a classified arm rather than an early return no headless test could
//! see.

use wolf_core::Liveness;

/// What pressing the Copy chord does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyPress {
    /// Issue the BLOCKING yank on the captive Neovim.
    Copy,
    /// Do nothing and issue no RPC; the string names why, as a mid-sentence clause
    /// (the [`crate::command_action`] / [`crate::nav_ui`] convention).
    Refuse(&'static str),
}

/// What pressing the Paste chord does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PastePress {
    /// Start the ASYNCHRONOUS clipboard read.
    Read,
    /// Do nothing and read nothing; the string names why, as a mid-sentence clause.
    Refuse(&'static str),
}

/// Whether the Copy chord may issue its yank at all, given the core's liveness
/// reading (or `None` when there is no core).
///
/// TOTAL by construction: a `match` over `Option<Liveness>` with **no `_` arm**
/// (see the module docs). Only [`Liveness::Alive`] yields [`CopyPress::Copy`];
/// every other input refuses, prints its reason, and issues no RPC.
#[must_use]
pub fn copy_decision(liveness: Option<Liveness>) -> CopyPress {
    todo!("clipboard_ui::copy_decision is not implemented yet (liveness: {liveness:?})")
}

/// Whether the Paste chord may start its clipboard read at all, given the core's
/// liveness reading (or `None` when there is no core).
///
/// TOTAL by construction, the same shape as [`copy_decision`] — but for a
/// different reason, which the module docs state and which must not be collapsed
/// into "both are blocking". Nothing on this path blocks.
#[must_use]
pub fn paste_decision(liveness: Option<Liveness>) -> PastePress {
    todo!("clipboard_ui::paste_decision is not implemented yet (liveness: {liveness:?})")
}

/// The text to write to the system clipboard for a completed yank, or `None` to
/// write NOTHING.
///
/// `None` is not "clear the clipboard" — see the module docs. `Some(text)` is the
/// selection passed through unchanged.
#[must_use]
pub fn clipboard_write(selection: Option<&str>) -> Option<&str> {
    todo!("clipboard_ui::clipboard_write is not implemented yet (selection: {selection:?})")
}

/// The text to hand `WolfCore::paste`, or `None` to queue no input event at all.
///
/// `read` is the flattened result of `gdk::Clipboard::read_text_async`: `None` for
/// an error the caller has already printed and for a clipboard holding no text.
#[must_use]
pub fn paste_payload(read: Option<&str>) -> Option<&str> {
    todo!("clipboard_ui::paste_payload is not implemented yet (read: {read:?})")
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;

    /// Every input both decisions are total over: the three readings a core can
    /// produce, plus the ABSENT core (`None`) the window's close handler leaves
    /// behind.
    const ALL_LIVENESS_INPUTS: [Option<Liveness>; 4] = [
        Some(Liveness::Alive),
        Some(Liveness::NotResponding),
        Some(Liveness::Dead),
        None,
    ];

    /// How many inputs each decision is total over, written as a LITERAL rather
    /// than as `ALL_LIVENESS_INPUTS.len()`, and that is the whole point of it —
    /// the `nav_ui::tests::SWEEP_INPUTS` precedent. Comparing a table's length to
    /// the length of the array the table was built from is a TAUTOLOGY: both
    /// shrink together, so the check passes while the sweep covers less than the
    /// domain it claims to.
    ///
    /// It is also the number that makes the `_`-free match legible from the test
    /// side. Four arms and four rows means every arm is measured; a fifth
    /// `Liveness` variant is an `E0004` in the seam and a red row count here.
    const NAMED_LIVENESS_INPUTS: usize = 4;

    /// Whether `reason` opens like a SENTENCE (`Xy…`) rather than as a
    /// mid-sentence clause. Copied from
    /// `command_action::tests::is_sentence_cased`, including its deliberate
    /// allowance for an ALL-CAPS opener such as `GNOME`.
    fn is_sentence_cased(reason: &str) -> bool {
        let mut chars = reason.chars();
        let (Some(first), Some(second)) = (chars.next(), chars.next()) else {
            return false;
        };
        first.is_uppercase() && second.is_lowercase()
    }

    // =========================================================================
    // copy_decision
    // =========================================================================

    /// What a [`CopyCase`] row expects [`copy_decision`] to return.
    ///
    /// A refusal is matched on its CLASS, not on its reason string: the reasons
    /// are documentation, and their real contract (non-empty, distinct,
    /// clause-shaped) is asserted by
    /// [`copy_and_paste_refusals_are_distinct_and_non_empty`] — the
    /// `buffer_picker::tests::WantOpen` precedent.
    #[derive(Clone, Copy)]
    enum WantCopy {
        /// `CopyPress::Copy` — the BLOCKING yank is issued.
        Copy,
        /// `CopyPress::Refuse(_)` — no RPC at all.
        Refuse,
    }

    /// One [`copy_decision`] table row.
    struct CopyCase {
        desc: &'static str,
        liveness: Option<Liveness>,
        want: WantCopy,
    }

    /// Pins the Copy decision for EVERY `Option<Liveness>` input.
    ///
    /// ## What it discriminates, and the edit that reddens it
    ///
    /// FALSIFYING EDIT: make the `Some(Liveness::NotResponding)` arm answer
    /// `CopyPress::Copy`. That is precisely the revert that puts a second full
    /// `rpc_timeout` budget of frozen GTK main loop on a core already known not to
    /// answer — every Ctrl+Shift+C after the first stall re-freezes the window.
    /// The `None` arm is the second one worth naming: a `Copy` there reaches for a
    /// core the window's close handler has already taken out.
    ///
    /// The row-count assertion against [`NAMED_LIVENESS_INPUTS`] is what makes the
    /// table non-vacuous. A row that quietly went missing cannot hide behind the
    /// three that remain, and the arm most likely to go missing is exactly the
    /// wedged one.
    #[test]
    fn copy_decision_table() {
        let cases = [
            CopyCase {
                desc: "a live core is the ONLY input that may issue the blocking yank",
                liveness: Some(Liveness::Alive),
                want: WantCopy::Copy,
            },
            CopyCase {
                desc: "a wedged core refuses: the yank is a blocking RPC against a core known not \
                       to answer",
                liveness: Some(Liveness::NotResponding),
                want: WantCopy::Refuse,
            },
            CopyCase {
                desc: "a dead core refuses: there is no selection to yank and an RPC against a \
                       dead handle is not reliably fast",
                liveness: Some(Liveness::Dead),
                want: WantCopy::Refuse,
            },
            CopyCase {
                desc: "no core at all (the close handler took it out) refuses",
                liveness: None,
                want: WantCopy::Refuse,
            },
        ];

        assert_eq!(
            cases.len(),
            NAMED_LIVENESS_INPUTS,
            "the copy_decision table must carry exactly one row per input the seam is total over: \
             {NAMED_LIVENESS_INPUTS} (three liveness readings plus the absent core), not {}. A \
             missing row means an arm nothing checks, and the arm most likely to go missing is \
             `NotResponding` — the one refusal that stops every later Ctrl+Shift+C re-blocking the \
             GTK main loop against a wedged Neovim.",
            cases.len()
        );
        assert_eq!(
            ALL_LIVENESS_INPUTS.len(),
            NAMED_LIVENESS_INPUTS,
            "ALL_LIVENESS_INPUTS carries {} entries but the seam is total over \
             {NAMED_LIVENESS_INPUTS}. If `wolf_core::Liveness` gained a variant, the `_`-free \
             matches in this module are an E0004 already and this constant is the second thing to \
             update; if an entry was DELETED, every sweep below silently covers less than the \
             domain it claims to.",
            ALL_LIVENESS_INPUTS.len()
        );

        for case in &cases {
            let got = copy_decision(case.liveness);
            match case.want {
                WantCopy::Copy => assert_eq!(
                    got,
                    CopyPress::Copy,
                    "case: {} — copy_decision({:?}) returned {got:?}, but this input MUST be Copy. \
                     A refusal here makes Ctrl+Shift+C do nothing at all on a perfectly healthy \
                     editor, and prints a reason for a hazard that is not present.",
                    case.desc,
                    case.liveness
                ),
                WantCopy::Refuse => assert!(
                    matches!(got, CopyPress::Refuse(_)),
                    "case: {} — copy_decision({:?}) returned {got:?}, but this input MUST refuse. \
                     A Copy here funnels through `WolfCore::copy_visual_selection` into \
                     `block_on_rpc` — a BLOCKING call on the GTK main thread — against a core that \
                     is known not to answer or that has already been taken out: every press then \
                     freezes the window for a full `rpc_timeout` budget, which is the exact \
                     regression this guard exists to prevent.",
                    case.desc,
                    case.liveness
                ),
            }
        }

        // The rule stated over the WHOLE domain rather than row by row, so a
        // failure names the rule instead of quoting an index — the
        // `buffer_picker::only_a_live_core_is_snapshotted` convention. This is
        // also what catches a table whose four rows became four copies of one.
        let accepting: Vec<Option<Liveness>> = ALL_LIVENESS_INPUTS
            .into_iter()
            .filter(|liveness| copy_decision(*liveness) == CopyPress::Copy)
            .collect();
        let only_alive = [Some(Liveness::Alive)];
        assert_eq!(
            accepting.as_slice(),
            only_alive.as_slice(),
            "exactly ONE of the {NAMED_LIVENESS_INPUTS} liveness inputs may issue the blocking \
             yank — a core reading Alive. An EXTRA entry means the yank is issued against a core \
             already known to be wedged, dead or absent, so every press after the first stall \
             re-freezes the main loop. An entry MISSING means Ctrl+Shift+C can never work at all, \
             since Alive is what a healthy editor always reports."
        );
    }

    // =========================================================================
    // paste_decision
    // =========================================================================

    /// What a [`PasteCase`] row expects [`paste_decision`] to return.
    #[derive(Clone, Copy)]
    enum WantPaste {
        /// `PastePress::Read` — the asynchronous clipboard read is started.
        Read,
        /// `PastePress::Refuse(_)` — nothing is read and nothing is queued.
        Refuse,
    }

    /// One [`paste_decision`] table row.
    struct PasteCase {
        desc: &'static str,
        liveness: Option<Liveness>,
        want: WantPaste,
    }

    /// Pins the Paste decision for EVERY `Option<Liveness>` input.
    ///
    /// ## What it discriminates, and the edit that reddens it
    ///
    /// FALSIFYING EDIT: make the `None` arm answer `PastePress::Read`. Nothing on
    /// this path blocks, so the hazard is not a frozen main loop — it is that the
    /// read is queued against a core the close handler has already taken out, and
    /// its callback lands on a shell with nowhere to deliver the text. The
    /// `Dead` / `NotResponding` arms carry the same shape one step earlier: an
    /// opaque overlay is up, the user cannot see what they are pasting into, and
    /// the text would arrive in a RESTARTED Neovim minutes later.
    ///
    /// **This test is the one that goes red if someone "simplifies" the paste gate
    /// away as redundant with the copy gate.** It is not redundant: the two seams
    /// refuse for reasons of different kinds, which is why the module doc says so
    /// outright and why this table exists separately rather than as four more rows
    /// of [`copy_decision_table`].
    #[test]
    fn paste_decision_table() {
        let cases = [
            PasteCase {
                desc: "a live core is the ONLY input that may start the clipboard read",
                liveness: Some(Liveness::Alive),
                want: WantPaste::Read,
            },
            PasteCase {
                desc: "a wedged core refuses: the stall overlay is up, so the paste would land in \
                       a buffer the user cannot see",
                liveness: Some(Liveness::NotResponding),
                want: WantPaste::Refuse,
            },
            PasteCase {
                desc: "a dead core refuses: the queued text would be delivered to a RESTARTED \
                       Neovim, not to the one the user was looking at",
                liveness: Some(Liveness::Dead),
                want: WantPaste::Refuse,
            },
            PasteCase {
                desc: "no core at all (the close handler took it out) refuses",
                liveness: None,
                want: WantPaste::Refuse,
            },
        ];

        assert_eq!(
            cases.len(),
            NAMED_LIVENESS_INPUTS,
            "the paste_decision table must carry exactly one row per input the seam is total over: \
             {NAMED_LIVENESS_INPUTS} (three liveness readings plus the absent core), not {}. A \
             missing row means an arm nothing checks.",
            cases.len()
        );

        for case in &cases {
            let got = paste_decision(case.liveness);
            match case.want {
                WantPaste::Read => assert_eq!(
                    got,
                    PastePress::Read,
                    "case: {} — paste_decision({:?}) returned {got:?}, but this input MUST be \
                     Read. A refusal here makes Ctrl+Shift+V do nothing at all on a perfectly \
                     healthy editor.",
                    case.desc,
                    case.liveness
                ),
                WantPaste::Refuse => assert!(
                    matches!(got, PastePress::Refuse(_)),
                    "case: {} — paste_decision({:?}) returned {got:?}, but this input MUST refuse. \
                     The hazard here is NOT a blocking call — nothing on the paste path blocks — \
                     so do not read this failure as a duplicate of copy_decision's. It is that an \
                     opaque crash or stall overlay is up, or the core is already gone: the text \
                     would be queued now and applied to a RESTARTED Neovim later, at whatever \
                     cursor position that one happens to have, with the user unable to see where \
                     it went.",
                    case.desc,
                    case.liveness
                ),
            }
        }

        // The whole-domain statement, exactly as `copy_decision_table` makes it.
        let accepting: Vec<Option<Liveness>> = ALL_LIVENESS_INPUTS
            .into_iter()
            .filter(|liveness| paste_decision(*liveness) == PastePress::Read)
            .collect();
        let only_alive = [Some(Liveness::Alive)];
        assert_eq!(
            accepting.as_slice(),
            only_alive.as_slice(),
            "exactly ONE of the {NAMED_LIVENESS_INPUTS} liveness inputs may start the clipboard \
             read — a core reading Alive. An EXTRA entry means a paste is queued against a core \
             that is wedged, dead or absent. An entry MISSING means Ctrl+Shift+V can never work at \
             all."
        );
    }

    // =========================================================================
    // The refusal strings
    // =========================================================================

    /// All six refusal reasons — three per direction — are non-empty, read as a
    /// mid-sentence clause, and are pairwise DISTINCT.
    ///
    /// The convention and the assertions are
    /// `command_action::unimplemented_reasons_are_distinct_and_non_empty`'s, and
    /// distinctness is the load-bearing half for the same reason it is there: the
    /// reason is the whole of what a reader sees when a chord appears to do
    /// nothing, and two arms sharing a string make "the editor is wedged"
    /// indistinguishable from "the editor has crashed" — two states with
    /// completely different routes back, one of which resolves by itself.
    ///
    /// Distinctness is asserted ACROSS the two directions as well as within each,
    /// which is the discriminating part: the refusals a user actually meets are
    /// printed as `copy not performed: {reason}` and `paste not performed:
    /// {reason}`, and giving Copy and Paste the same wedged-core sentence would
    /// make a Ctrl+Shift+V that did nothing read exactly like a Ctrl+Shift+C that
    /// did nothing.
    ///
    /// FALSIFYING EDIT: give `Some(Liveness::Dead)` and
    /// `Some(Liveness::NotResponding)` the same string on either side.
    #[test]
    fn copy_and_paste_refusals_are_distinct_and_non_empty() {
        let probes = [
            ("a wedged (NotResponding) core", Some(Liveness::NotResponding)),
            ("a dead core", Some(Liveness::Dead)),
            ("an absent core, after shutdown took it out", None),
        ];

        let mut seen: HashSet<&'static str> = HashSet::new();

        for (source, liveness) in probes {
            let copy = copy_decision(liveness);
            let CopyPress::Refuse(copy_reason) = copy else {
                panic!(
                    "copy_decision for {source} produced {copy:?} rather than a refusal, so this \
                     test cannot check the reason it exists to check. Either that arm now issues \
                     the blocking yank (in which case `copy_decision_table` is also red, and that \
                     is the finding) or the probe no longer selects the arm it names."
                )
            };
            let paste = paste_decision(liveness);
            let PastePress::Refuse(paste_reason) = paste else {
                panic!(
                    "paste_decision for {source} produced {paste:?} rather than a refusal, so this \
                     test cannot check the reason it exists to check. Either that arm now starts \
                     the clipboard read (in which case `paste_decision_table` is also red, and \
                     that is the finding) or the probe no longer selects the arm it names."
                )
            };

            for (direction, reason) in [("copy", copy_reason), ("paste", paste_reason)] {
                assert!(
                    !reason.trim().is_empty(),
                    "the {direction} refusal reason for {source} is empty. It is printed as \
                     \"wolf-gnome: {direction} not performed: {{reason}}\", so an empty one makes \
                     a deliberate refusal indistinguishable from a chord that silently did nothing."
                );
                assert!(
                    !is_sentence_cased(reason),
                    "the {direction} refusal reason for {source} is sentence-cased ({reason:?}). \
                     It is interpolated MID-SENTENCE into the printed diagnostic, exactly as \
                     `command_action.rs`'s, `buffer_picker.rs`'s and `nav_ui.rs`'s reasons are, so \
                     it must start lowercase (an ALL-CAPS proper noun such as \"GNOME\" is fine)."
                );
                assert!(
                    seen.insert(reason),
                    "the {direction} refusal reason for {source} ({reason:?}) duplicates another \
                     arm's reason. All six must be DISTINCT so the printed line names the real \
                     blocker: a wedged Neovim is recoverable and a dead one is not, an absent core \
                     means the window is already tearing down — and the copy and paste halves \
                     refuse for reasons of different KINDS, one about a blocking RPC and one about \
                     text arriving in a restarted editor. One shared string collapses two of those \
                     into a message that cannot be acted on."
                );
            }
        }

        assert_eq!(
            seen.len(),
            probes.len() * 2,
            "the six probed refusals produced {} distinct reason(s), not {}. See the per-probe \
             message above for which pair collapsed.",
            seen.len(),
            probes.len() * 2
        );
    }

    // =========================================================================
    // What crosses the boundary
    // =========================================================================

    /// One row of either payload table: the same three shapes apply to both
    /// directions, so they share a type.
    struct PayloadCase {
        desc: &'static str,
        input: Option<&'static str>,
        expected: Option<&'static str>,
    }

    /// A selection with real content, used in both payload tables so a
    /// pass-through row is never confused with a rejection row.
    ///
    /// Deliberately NOT a word either seam could plausibly synthesize, and
    /// deliberately several words long with interior spaces, so a result that
    /// fabricated, trimmed or truncated text rather than passing the argument
    /// through is visible in the failure message.
    const REAL_SELECTION: &str = "a real yanked selection";

    /// The rows both payload rules must satisfy, stated once.
    ///
    /// Row 3 is what stops rows 1 and 2 being vacuous: a function that answered
    /// `None` for everything would satisfy both refusal rows while making Copy
    /// write nothing and Paste queue nothing, forever. Row 4 pins that the text is
    /// passed THROUGH rather than normalized — a trim or a line-ending rewrite
    /// here would silently change what the user copied.
    const PAYLOAD_CASES: &[PayloadCase] = &[
        PayloadCase {
            desc: "nothing at all: there is no text, so there is nothing to hand on",
            input: None,
            expected: None,
        },
        PayloadCase {
            desc: "the EMPTY string: real, but empty, so it is refused as firmly as None",
            input: Some(""),
            expected: None,
        },
        PayloadCase {
            desc: "real text is passed THROUGH unchanged",
            input: Some(REAL_SELECTION),
            expected: Some(REAL_SELECTION),
        },
        PayloadCase {
            desc: "a multi-line selection is passed through verbatim, newlines and all",
            input: Some("first\nsecond\n"),
            expected: Some("first\nsecond\n"),
        },
    ];

    /// [`clipboard_write`]: an absent or empty selection writes NOTHING, and real
    /// text is passed through unchanged.
    ///
    /// ## What it discriminates
    ///
    /// FALSIFYING EDIT: return `selection` unchanged. `Some("")` then leaks
    /// through and the front-end writes an empty string to the system clipboard —
    /// which is worse than it sounds, because it silently destroys whatever the
    /// user had copied from another application in response to a Ctrl+Shift+C that
    /// yanked nothing.
    ///
    /// The complementary edit — return `None` unconditionally — is what the
    /// pass-through rows exclude; without them a seam that wrote nothing ever
    /// would satisfy this test forever.
    ///
    /// The final assertion states the rule that is not visible in any row: a
    /// `None` answer means WRITE NOTHING, never CLEAR. No signature can carry
    /// that, and the caller is where it can be got wrong, so it is written down
    /// where the next reader of this test will meet it.
    #[test]
    fn an_absent_or_empty_selection_writes_nothing() {
        for case in PAYLOAD_CASES {
            assert_eq!(
                clipboard_write(case.input),
                case.expected,
                "clipboard_write({:?}) must be {:?} — {}. An empty selection reaching the \
                 clipboard overwrites whatever the user copied elsewhere with nothing; a real \
                 selection that does NOT reach it makes Copy silently do nothing at all.",
                case.input,
                case.expected,
                case.desc
            );
        }

        assert!(
            PAYLOAD_CASES
                .iter()
                .any(|case| case.expected.is_some() && case.input == case.expected),
            "no PAYLOAD_CASES row asserts that real text is passed through, so a `clipboard_write` \
             that answered None for every input would satisfy every row above while making Copy \
             permanently dead. The pass-through rows are what make the refusal rows mean anything."
        );

        // The rule the signature cannot state, recorded where it is read.
        //
        // `clipboard_write(None) == None` is an instruction to the caller to
        // perform NO write. `gdk::Clipboard` also offers `set_text("")`, which is
        // the call a reader reaches for on seeing an "empty" answer — and it
        // CLEARS the selection, discarding whatever the user copied from another
        // application. There is no assertion that can catch that from here: this
        // seam returns data, and the mistake is made by `shell::copy_selection`
        // with the data in hand. See the module docs.
    }

    /// [`paste_payload`]: an absent or empty clipboard read pastes NOTHING, and
    /// real text is passed through unchanged.
    ///
    /// ## What it discriminates
    ///
    /// FALSIFYING EDIT: return `read` unchanged. `Some("")` then leaks through,
    /// `WolfCore::paste` queues an `InputEvent::Paste` carrying nothing, and the
    /// captive Neovim performs an `nvim_paste` of the empty string — an undo entry
    /// in the user's buffer in exchange for no text. The complementary edit —
    /// return `None` unconditionally — is excluded by the pass-through rows, the
    /// same way it is for [`clipboard_write`].
    ///
    /// The two rules are asserted separately rather than through one shared
    /// function on purpose. They are the same table today, and they are two
    /// independent decisions: nothing says the outbound rule and the inbound rule
    /// have to stay identical, and folding them into one call site would mean a
    /// later change to either silently changed both.
    #[test]
    fn an_absent_or_empty_clipboard_read_pastes_nothing() {
        for case in PAYLOAD_CASES {
            assert_eq!(
                paste_payload(case.input),
                case.expected,
                "paste_payload({:?}) must be {:?} — {}. An empty payload reaching \
                 `WolfCore::paste` queues an InputEvent for no text and leaves an undo entry \
                 behind; a real payload that does NOT reach it makes Ctrl+Shift+V silently do \
                 nothing, which is indistinguishable from an empty clipboard.",
                case.input,
                case.expected,
                case.desc
            );
        }

        assert!(
            PAYLOAD_CASES
                .iter()
                .any(|case| case.expected.is_some() && case.input == case.expected),
            "no PAYLOAD_CASES row asserts that real text is passed through, so a `paste_payload` \
             that answered None for every input would satisfy every row above while making Paste \
             permanently dead."
        );
    }
}
