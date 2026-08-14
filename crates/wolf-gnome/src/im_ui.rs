//! The pure, GTK-free rulebook for COMPOSED and INPUT-METHOD text entry in
//! `wolf-gnome`: what one window-level key event is routed to, and what one
//! `commit` string from that window's `gtk::IMMulticontext` becomes.
//!
//! The input-method counterpart of [`crate::nav_ui`] and [`crate::clipboard_ui`].
//! Data in, data out — no widget type, no `gtk::IMContext`, no `WolfCore` — which
//! is exactly what lets the whole rulebook be unit-tested headlessly, the same way
//! [`crate::command_action`], [`crate::tabs_ui`] and [`crate::cursor`] keep their
//! decisions pure. The widget work of carrying the answers out lives in
//! [`crate::window`], which owns the context, the key controller and the
//! `filter_keypress` call.
//!
//! ## Offering a key to the input method is a SIDE EFFECT
//!
//! `gtk_im_context_filter_keypress` is not a query. A `dead_acute` it accepts is
//! swallowed into a PENDING compose state that commits on some later key, so a key
//! offered to the context while an overlay is open cannot be un-offered — the
//! accent it started reappears inside whatever the window forwards next. The
//! overlay guard must therefore short-circuit AHEAD of the offer, and
//! [`key_route`]'s early return is that precedence written as an executable value
//! rather than as a comment at one call site.
//!
//! ## Why the seam cannot be one call
//!
//! The input method's answer is not known until it has been asked, and asking is
//! the side effect above. Hence the `Option<bool>` staging: the glue asks
//! [`key_route`] once with `None`, and only an answer of
//! [`KeyRoute::AskInputMethod`] licenses it to call `filter_keypress` and ask again
//! with `Some(answer)`. A caller that consulted the context first and this seam
//! second has already spent the side effect, and no answer here can take it back.
//!
//! ## A control character must never become a `Key::Char`
//!
//! [`wolf_core::key_to_notation`] renders a modifier-free `Key::Char(c)` as the
//! bare character, so `Key::Char('\u{1b}')` sends a raw ESC byte through
//! `nvim_input` and leaves insert mode; `'\r'` and `'\t'` are Enter and Tab by the
//! same route. Those keys already HAVE a route: [`crate::gdk_key`]'s named-key
//! table maps `Return`, `Escape` and `Tab` to `Key::Enter`, `Key::Esc` and
//! `Key::Tab`, which `key_to_notation` renders as `<CR>`, `<Esc>` and `<Tab>`, and
//! which the input method leaves alone. A control character inside a commit would
//! be a SECOND, unnamed route to the same effect, with no `<lt>`-style escaping and
//! no modifier handling. [`crate::gdk_key`] already refuses a control character at
//! its own entry point; [`commit_keys`] states the identical rule at the second
//! entry point, and two entry points that disagree is the bug.
//!
//! ## Every committed key is modifier-free
//!
//! The committed text already carries the glyph, so `shift` is false on every
//! [`wolf_core::KeyEvent`] emitted here — the shift-glyph contract, which
//! [`crate::input`] owns and which is deliberately not restated. `ctrl`, `alt` and
//! `meta` are false for a stronger reason: a commit is a RESULT, not a chord. The
//! modifiers that produced it were spent inside the compose sequence or inside the
//! engine, so re-attaching them would turn text composed over `AltGr` into `<A-…>`
//! chords that Neovim runs as commands.
//!
//! ## The overlay refusal is not dead code
//!
//! GTK's built-in simple context emits `commit` SYNCHRONOUSLY from inside
//! `filter_keypress`, and the glue makes that call only where no overlay is open —
//! so this front-end's own glue cannot reach [`CommitDecision::Refuse`]. An
//! out-of-process input method (ibus, fcitx) commits ASYNCHRONOUSLY, on a bus
//! reply, at an arbitrary main-loop turn, exactly as
//! `gdk::Clipboard::read_text_async` resolves on a later turn. That arrival is what
//! the guard is for, and this paragraph exists so it is not deleted as unreachable.
//!
//! ## Totality is the contract
//!
//! [`key_route`] matches `Option<bool>` with **no `_` arm**, and the inner match
//! over [`KeyPhase`] carries none either, so a third phase is an `E0004` compile
//! error here until it is classified — the same guard
//! [`crate::nav_ui::nav_press`] and [`crate::clipboard_ui`] rely on. The phase is
//! an argument rather than two separate functions precisely so the RELEASE answer
//! is a classified arm, which is what keeps a release from ever being translated: a
//! translated release sends a second copy of every key the user types.

use wolf_core::{Key, KeyEvent};

/// Which of `gtk::EventControllerKey`'s two signals is being routed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyPhase {
    /// The `key-pressed` signal.
    Press,
    /// The `key-released` signal.
    Release,
}

/// What the glue does with ONE window-level key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyRoute {
    /// Forward nothing and — crucially — OFFER nothing: no `filter_keypress` call
    /// at all, and `glib::Propagation::Proceed` so the open overlay's own widgets
    /// keep the key.
    Overlay,
    /// Call `filter_keypress` with the controller's current event, then re-ask
    /// [`key_route`] with `Some(answer)`. The only arm that licenses the side
    /// effect.
    AskInputMethod,
    /// The input method claimed the event: `glib::Propagation::Stop`, translate
    /// nothing, and count nothing — the keystroke counter records keys DELIVERED to
    /// Neovim, and this one delivered nothing.
    Consumed,
    /// Hand the keyval and modifier state to [`crate::gdk_key`] and forward what it
    /// answers. A press-only answer.
    Translate,
    /// A RELEASE the input method declined: `glib::Propagation::Proceed`, and
    /// nothing else. **Never [`KeyRoute::Translate`]** — see the module docs on
    /// totality.
    Ignore,
}

/// What routing one window-level key event does, given its phase, whether an
/// overlay is open, and the input method's answer (`None` before it has been
/// asked).
///
/// The rule, total over every `(phase, overlay_open, im_filtered)`:
///
/// | `overlay_open` | `im_filtered` | `phase` | answer |
/// | --- | --- | --- | --- |
/// | `true` | any | any | [`KeyRoute::Overlay`] |
/// | `false` | `None` | any | [`KeyRoute::AskInputMethod`] |
/// | `false` | `Some(true)` | any | [`KeyRoute::Consumed`] |
/// | `false` | `Some(false)` | `Press` | [`KeyRoute::Translate`] |
/// | `false` | `Some(false)` | `Release` | [`KeyRoute::Ignore`] |
///
/// The overlay row is an EARLY RETURN, and its position is the whole precedence:
/// it holds whatever `im_filtered` says, so a caller that offered the key to the
/// input method anyway is told to discard the answer rather than act on it. Both
/// matches below it are `_`-free (see the module docs).
#[must_use]
pub fn key_route(phase: KeyPhase, overlay_open: bool, im_filtered: Option<bool>) -> KeyRoute {
    // FIRST, and before anything may be offered to the input method: a key
    // offered to it is a key that cannot be taken back.
    if overlay_open {
        return KeyRoute::Overlay;
    }

    match im_filtered {
        None => KeyRoute::AskInputMethod,
        Some(true) => KeyRoute::Consumed,
        Some(false) => match phase {
            KeyPhase::Press => KeyRoute::Translate,
            KeyPhase::Release => KeyRoute::Ignore,
        },
    }
}

/// The key events one `commit` string becomes: one modifier-free `Key::Char` per
/// `char`, in source order, with every control character DROPPED.
///
/// Empty text — and text that is nothing but control characters — yields an empty
/// vec, which is a routine answer rather than an error. See the module docs for why
/// a control character must never survive this function, and why none of the four
/// modifier flags is ever set.
///
/// Public rather than private so the rule can be asserted on its own, following
/// [`crate::nav_ui::nav_query_win`]'s precedent.
#[must_use]
pub fn commit_keys(text: &str) -> Vec<KeyEvent> {
    text.chars()
        .filter(|c| !c.is_control())
        .map(|c| KeyEvent {
            key: Key::Char(c),
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        })
        .collect()
}

/// What one `commit` from a window's input method does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitDecision {
    /// Send each event as a `wolf_core::InputEvent::Key`, in this order. Possibly
    /// EMPTY, which sends nothing and prints nothing.
    Send(Vec<KeyEvent>),
    /// Forward nothing; the string names why, as a mid-sentence clause (the
    /// [`crate::command_action`] / [`crate::nav_ui`] convention).
    Refuse(&'static str),
}

/// Whether a committed string may be forwarded to core at all.
///
/// An open overlay refuses, whatever the text. Everything else answers
/// [`CommitDecision::Send`] carrying [`commit_keys`], which may be an empty vec: an
/// empty or control-only commit is a routine event, and a refusal exists to be
/// PRINTED, so making one out of it would put a diagnostic line in front of a
/// reader for something that is not a hazard. The overlay case is the hazard, and
/// the module docs record why that arm is reachable at all.
#[must_use]
pub fn commit_decision(overlay_open: bool, text: &str) -> CommitDecision {
    if overlay_open {
        return CommitDecision::Refuse(
            "an overlay is open over the editor, so the text belongs to that overlay's own entry \
             rather than to the buffer behind it",
        );
    }
    CommitDecision::Send(commit_keys(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    use wolf_core::key_to_notation;

    // =========================================================================
    // The whole input domain
    // =========================================================================
    //
    // Two of the rules below are stated over EVERY input rather than over one
    // table row, so that a failure NAMES the rule instead of quoting an index —
    // the [`crate::nav_ui`] / [`crate::liveness_ui`] convention. They are the two
    // whose violation is catastrophic and easy to reintroduce: an overlay that
    // stops winning, and a release that starts being translated.

    /// Both phases [`key_route`] is total over.
    const ALL_PHASES: [KeyPhase; 2] = [KeyPhase::Press, KeyPhase::Release];

    /// Both overlay states. `false` first, so a sweep that silently stopped at the
    /// first entry would cover the arm that does NOT short-circuit.
    const ALL_OVERLAY: [bool; 2] = [false, true];

    /// Every shape the input method's answer can take: not yet asked (`None`),
    /// asked and CLAIMED, asked and DECLINED.
    const ALL_FILTER: [Option<bool>; 3] = [None, Some(true), Some(false)];

    /// How many inputs [`key_route`] is total over: two phases times two overlay
    /// states times three input-method answers.
    ///
    /// Written as LITERALS rather than as the product of those three arrays' own
    /// lengths, and that is the whole point of it — the
    /// `nav_ui::tests::SWEEP_INPUTS` and `NAMED_BUTTON_CASES` precedent. Counting
    /// iterations of loops over the same three arrays and comparing the answer to
    /// their own product is a TAUTOLOGY: an array that lost an entry shrinks both
    /// sides together, so the check passes while the sweep silently covers less
    /// than the domain it claims to.
    const ROUTE_CASES: usize = 2 * 2 * 3;

    /// How many inputs the overlay rule is stated over: two phases times three
    /// input-method answers, all at `overlay_open == true`. A literal, for
    /// [`ROUTE_CASES`]'s reason.
    const OVERLAY_SWEEP_INPUTS: usize = 2 * 3;

    /// How many inputs the release rule is stated over: two overlay states times
    /// three input-method answers, all at [`KeyPhase::Release`]. A literal, for
    /// [`ROUTE_CASES`]'s reason.
    const RELEASE_SWEEP_INPUTS: usize = 2 * 3;

    // =========================================================================
    // key_route
    // =========================================================================

    /// One [`key_route`] table row.
    struct RouteCase {
        desc: &'static str,
        phase: KeyPhase,
        overlay_open: bool,
        im_filtered: Option<bool>,
        expected: KeyRoute,
    }

    /// Pins the route for EVERY `(phase, overlay_open, im_filtered)` input.
    ///
    /// The row most likely to be "simplified" away is
    /// `(Release, false, Some(false)) => Ignore`: an inner match over the phase
    /// looks redundant beside the press arm, and collapsing it to `Translate`
    /// sends a second copy of every key the user types. The six overlay rows are
    /// the other half — they are what say the early return holds whatever the
    /// input method answered.
    #[test]
    fn key_route_table() {
        let cases = [
            // --- No overlay, not yet asked: the only arm that licenses the side
            // --- effect, and it licenses it for BOTH phases -------------------
            RouteCase {
                desc: "a press nobody has asked the input method about yet",
                phase: KeyPhase::Press,
                overlay_open: false,
                im_filtered: None,
                expected: KeyRoute::AskInputMethod,
            },
            RouteCase {
                desc: "a RELEASE is offered too, which is what `set_im_context` would do",
                phase: KeyPhase::Release,
                overlay_open: false,
                im_filtered: None,
                expected: KeyRoute::AskInputMethod,
            },
            // --- No overlay, the input method CLAIMED it ----------------------
            RouteCase {
                desc: "a claimed press is consumed: no translation, no keystroke counted",
                phase: KeyPhase::Press,
                overlay_open: false,
                im_filtered: Some(true),
                expected: KeyRoute::Consumed,
            },
            RouteCase {
                desc: "a claimed release is consumed too, so the pending sequence keeps it",
                phase: KeyPhase::Release,
                overlay_open: false,
                im_filtered: Some(true),
                expected: KeyRoute::Consumed,
            },
            // --- No overlay, the input method DECLINED: the phase decides ------
            RouteCase {
                desc: "a declined PRESS is the only input that reaches gdk_key",
                phase: KeyPhase::Press,
                overlay_open: false,
                im_filtered: Some(false),
                expected: KeyRoute::Translate,
            },
            RouteCase {
                desc: "a declined RELEASE is dropped — translating it doubles every key",
                phase: KeyPhase::Release,
                overlay_open: false,
                im_filtered: Some(false),
                expected: KeyRoute::Ignore,
            },
            // --- An open overlay wins, whatever the other two axes say ---------
            RouteCase {
                desc: "an overlay short-circuits a press BEFORE the input method is asked",
                phase: KeyPhase::Press,
                overlay_open: true,
                im_filtered: None,
                expected: KeyRoute::Overlay,
            },
            RouteCase {
                desc: "an overlay short-circuits a release before the offer as well",
                phase: KeyPhase::Release,
                overlay_open: true,
                im_filtered: None,
                expected: KeyRoute::Overlay,
            },
            RouteCase {
                desc: "a caller that asked anyway is told to DISCARD a claimed press",
                phase: KeyPhase::Press,
                overlay_open: true,
                im_filtered: Some(true),
                expected: KeyRoute::Overlay,
            },
            RouteCase {
                desc: "and to discard a claimed release",
                phase: KeyPhase::Release,
                overlay_open: true,
                im_filtered: Some(true),
                expected: KeyRoute::Overlay,
            },
            RouteCase {
                desc: "a declined press behind an overlay is still not translated",
                phase: KeyPhase::Press,
                overlay_open: true,
                im_filtered: Some(false),
                expected: KeyRoute::Overlay,
            },
            RouteCase {
                desc: "a declined release behind an overlay likewise",
                phase: KeyPhase::Release,
                overlay_open: true,
                im_filtered: Some(false),
                expected: KeyRoute::Overlay,
            },
        ];

        assert_eq!(
            cases.len(),
            ROUTE_CASES,
            "the key_route table must carry exactly {ROUTE_CASES} rows — one per \
             (phase, overlay_open, im_filtered) input the seam is total over — but carries {}. A \
             missing row means an arm nothing checks, and the arm most likely to go missing is the \
             declined RELEASE, whose answer is the only thing standing between the user and a \
             second copy of every keystroke.",
            cases.len()
        );

        // Every domain input appears EXACTLY once, so the count above cannot be
        // satisfied by twelve copies of one row, and a duplicated row cannot hide a
        // missing one. The nav_ui `BUTTON_CASES` coverage loop, tightened to an
        // exact count because this domain is small enough to enumerate whole.
        let inputs: Vec<(KeyPhase, bool, Option<bool>)> = cases
            .iter()
            .map(|case| (case.phase, case.overlay_open, case.im_filtered))
            .collect();
        for phase in ALL_PHASES {
            for overlay_open in ALL_OVERLAY {
                for im_filtered in ALL_FILTER {
                    let probe = (phase, overlay_open, im_filtered);
                    let hits = inputs.iter().filter(|input| **input == probe).count();
                    assert_eq!(
                        hits, 1,
                        "the input {probe:?} appears {hits} time(s) in the key_route table, not \
                         once. Zero means that arm is unmeasured while the row count still adds \
                         up (some other input is doubled); more than one means the table is \
                         padded and a genuinely missing input is being hidden by the duplicate."
                    );
                }
            }
        }

        for case in &cases {
            let got = key_route(case.phase, case.overlay_open, case.im_filtered);
            assert_eq!(
                got, case.expected,
                "key_route({:?}, {}, {:?}) must be {:?}, got {got:?} — {}. Each answer is an \
                 obligation on the glue: Overlay forwards and offers NOTHING, AskInputMethod is \
                 the only licence to call `filter_keypress`, Consumed stops propagation and \
                 translates nothing, Translate hands the keyval to `gdk_key`, and Ignore drops a \
                 release. A wrong answer here is a wrong side effect there.",
                case.phase, case.overlay_open, case.im_filtered, case.expected, case.desc
            );
        }

        // Non-vacuity of a different kind: every variant is REACHABLE. An
        // implementation that collapsed two arms into one would satisfy a table
        // whose rows had been "corrected" alongside it, but not this.
        let answers: Vec<KeyRoute> = ALL_PHASES
            .into_iter()
            .flat_map(|phase| {
                ALL_OVERLAY.into_iter().flat_map(move |overlay_open| {
                    ALL_FILTER
                        .into_iter()
                        .map(move |im_filtered| key_route(phase, overlay_open, im_filtered))
                })
            })
            .collect();
        for variant in [
            KeyRoute::Overlay,
            KeyRoute::AskInputMethod,
            KeyRoute::Consumed,
            KeyRoute::Translate,
            KeyRoute::Ignore,
        ] {
            assert!(
                answers.contains(&variant),
                "no input in the whole {ROUTE_CASES}-input domain produces {variant:?}, so that \
                 route is dead. Either two arms have been collapsed into one — which silently \
                 changes what the glue does with a whole class of keys — or the variant is new \
                 and unclassified, in which case `key_route`'s `_`-free matches are an E0004 and \
                 this list is the second thing to update."
            );
        }
    }

    /// An open overlay wins over the input method, for every phase and every
    /// answer — including an answer the caller had already obtained.
    ///
    /// Stated as a SWEEP rather than as table rows because the rule, not the row,
    /// is what a reader needs named when it breaks: moving the early return below
    /// the `match im_filtered` is a one-line "simplification" that reads as tidier
    /// and offers every key to the context while the picker, palette, buffer picker
    /// or project search is up. That offer cannot be taken back — a `dead_acute` the
    /// context accepts reappears inside whatever the window forwards next.
    #[test]
    fn an_open_overlay_wins_over_the_input_method() {
        let mut probed = 0_usize;

        for phase in ALL_PHASES {
            for im_filtered in ALL_FILTER {
                probed += 1;
                let got = key_route(phase, true, im_filtered);
                assert_eq!(
                    got,
                    KeyRoute::Overlay,
                    "key_route({phase:?}, true, {im_filtered:?}) answered {got:?}. With an overlay \
                     open the answer must be Overlay for EVERY input, because the answer's whole \
                     job is to short-circuit ahead of `filter_keypress`: offering a key to the \
                     input method is a side effect, not a query. An answer of AskInputMethod here \
                     means the guard now sits BELOW the offer and the overlay's keystrokes are \
                     being fed into a compose state the user cannot see; Consumed, Translate or \
                     Ignore mean a caller that asked anyway is being told to act on the answer \
                     rather than discard it."
                );
            }
        }

        assert_eq!(
            probed,
            OVERLAY_SWEEP_INPUTS,
            "the overlay sweep visited {probed} inputs, not the {OVERLAY_SWEEP_INPUTS} it claims \
             to cover ({} phase(s) x {} input-method answer(s)). A shrunk domain states the rule \
             over less than the inputs it is meant to hold for.",
            ALL_PHASES.len(),
            ALL_FILTER.len()
        );

        // Non-vacuity: `|_, _, _| KeyRoute::Overlay` satisfies every iteration above
        // while making the window permanently deaf. The witness is the input the
        // glue takes on an ordinary keystroke.
        let no_overlay = key_route(KeyPhase::Press, false, Some(false));
        assert_eq!(
            no_overlay,
            KeyRoute::Translate,
            "with NO overlay open, a declined press answered {no_overlay:?} rather than Translate, \
             so the implication swept above holds for the wrong reason: a seam that answered \
             Overlay unconditionally would pass every assertion in this test while routing nothing \
             to Neovim at all. `key_route_table` is red alongside this."
        );
    }

    /// A release is never translated, for any overlay state and any answer.
    ///
    /// A sweep rather than a row for [`an_open_overlay_wins_over_the_input_method`]'s
    /// reason. The failure this states is not subtle in effect and is very subtle in
    /// source: `Some(false) => Translate` without the inner match over the phase
    /// sends every key twice, once on the press and once on the release, which reads
    /// as "the editor types everything double" and names nothing.
    #[test]
    fn a_release_is_never_translated() {
        let mut probed = 0_usize;

        for overlay_open in ALL_OVERLAY {
            for im_filtered in ALL_FILTER {
                probed += 1;
                let got = key_route(KeyPhase::Release, overlay_open, im_filtered);
                assert_ne!(
                    got,
                    KeyRoute::Translate,
                    "key_route(Release, {overlay_open}, {im_filtered:?}) answered Translate. A \
                     release must NEVER reach `gdk_key`: the press for the same physical key was \
                     already translated and forwarded, so translating the release forwards a \
                     SECOND copy of it. Every word the user types arrives doubled, and the \
                     symptom names the keyboard rather than this seam. The right answers are \
                     Overlay behind an overlay, Consumed when the input method claimed it, and \
                     Ignore when it declined."
                );
            }
        }

        assert_eq!(
            probed,
            RELEASE_SWEEP_INPUTS,
            "the release sweep visited {probed} inputs, not the {RELEASE_SWEEP_INPUTS} it claims \
             to cover ({} overlay state(s) x {} input-method answer(s)).",
            ALL_OVERLAY.len(),
            ALL_FILTER.len()
        );

        // Non-vacuity: a seam that answered Ignore for everything would satisfy the
        // sweep above and drop every keystroke on the floor.
        let press = key_route(KeyPhase::Press, false, Some(false));
        assert_eq!(
            press,
            KeyRoute::Translate,
            "a declined PRESS answered {press:?} rather than Translate, so the rule swept above is \
             vacuous: `Translate` is produced by no input at all and the sweep would hold for a \
             seam that had stopped routing keys to Neovim entirely."
        );
    }

    /// The two-stage ask cannot loop: the input method is asked before it answers,
    /// and never after.
    ///
    /// ## Why this is two one-directional statements rather than a biconditional
    ///
    /// "AskInputMethod is the answer exactly where `im_filtered` is `None`" is
    /// FALSE against a correct seam, and false in the direction the design depends
    /// on: `key_route(Press, true, None)` answers [`KeyRoute::Overlay`], because the
    /// overlay early return precedes the match — which is precisely what
    /// [`an_open_overlay_wins_over_the_input_method`] demands. So the rule is
    /// stated as the two implications that are true, each with its own non-vacuity
    /// witness, and the witness for the deliberately-unasserted converse is
    /// asserted at the end so nobody "completes" the equivalence and reddens the
    /// overlay rule instead.
    ///
    /// What the pair protects is that the glue's re-ask terminates: the second
    /// call, made with `Some(answer)`, can never come back asking again.
    #[test]
    fn the_input_method_is_asked_exactly_once_before_it_answers() {
        let mut probed = 0_usize;
        let mut asked = 0_usize;
        let mut unasked_without_overlay = 0_usize;

        for phase in ALL_PHASES {
            for overlay_open in ALL_OVERLAY {
                for im_filtered in ALL_FILTER {
                    probed += 1;
                    let got = key_route(phase, overlay_open, im_filtered);

                    // Direction 1: an answer of AskInputMethod implies nobody has
                    // asked yet. This is the one that keeps the re-ask from looping.
                    if got == KeyRoute::AskInputMethod {
                        asked += 1;
                        assert_eq!(
                            im_filtered, None,
                            "key_route({phase:?}, {overlay_open}, {im_filtered:?}) answered \
                             AskInputMethod even though the input method has ALREADY answered. The \
                             glue asks once with None and re-asks with Some(answer); telling it to \
                             ask again after the answer is in hand is an infinite loop in \
                             `key-pressed`, with `filter_keypress` — a side-effecting call that \
                             swallows keys into a pending compose state — inside it."
                        );
                    }

                    // Direction 2: no overlay and no answer yet implies the offer is
                    // licensed. Without it, the glue would never call
                    // `filter_keypress` at all and no accent would ever compose.
                    if im_filtered.is_none() && !overlay_open {
                        unasked_without_overlay += 1;
                        assert_eq!(
                            got,
                            KeyRoute::AskInputMethod,
                            "key_route({phase:?}, false, None) answered {got:?} rather than \
                             AskInputMethod. That arm is the ONLY licence the glue has to call \
                             `filter_keypress`, so any other answer means the input method is \
                             never offered a key: dead keys, compose sequences and every input \
                             method reach Neovim as nothing, which is the exact defect this seam \
                             exists to close."
                        );
                    }
                }
            }
        }

        assert_eq!(
            probed, ROUTE_CASES,
            "the sweep visited {probed} inputs, not the {ROUTE_CASES} the seam is total over. \
             Whichever of the three domain arrays is not 2, 2 and 3 is the one that changed."
        );
        assert!(
            asked > 0,
            "NOT ONE of the {probed} inputs answered AskInputMethod, so direction 1 held \
             vacuously. A seam that never asks the input method anything would pass it while \
             making every composed character unreachable."
        );
        assert!(
            unasked_without_overlay > 0,
            "NOT ONE of the {probed} inputs paired `im_filtered == None` with no overlay, so \
             direction 2 held vacuously — the domain arrays no longer contain the input the glue \
             takes on every ordinary keystroke."
        );

        // The witness for the converse this test deliberately does NOT assert.
        // Recorded as an assertion rather than as prose, because the tempting
        // "tightening" — AskInputMethod exactly where `im_filtered` is None — is
        // false here and would put the overlay guard back below the offer.
        let behind_an_overlay = key_route(KeyPhase::Press, true, None);
        assert_eq!(
            behind_an_overlay,
            KeyRoute::Overlay,
            "the witness for the deliberately-unasserted converse is broken: \
             key_route(Press, true, None) answered {behind_an_overlay:?} rather than Overlay. An \
             unasked press behind an OPEN overlay must NOT be routed to the input method, which is \
             why direction 2 above carries its `!overlay_open` clause instead of being written as \
             a biconditional."
        );
    }

    // =========================================================================
    // commit_keys
    // =========================================================================

    /// One [`commit_keys`] fixture.
    struct CommitCase {
        desc: &'static str,
        /// The string an input method commits.
        text: &'static str,
        /// The characters that must reach Neovim, in order, spelled as a literal
        /// rather than derived from [`CommitCase::text`] — an expectation computed
        /// by the rule under test measures nothing.
        survivors: &'static str,
    }

    /// The commit corpus. Every rule below is stated over this same set of
    /// fixtures, so a row added here strengthens four tests at once.
    ///
    /// The three composed characters are the ones the harness measured GTK's own
    /// compose table producing (`dead_acute` then `e`; `Multi_key a e`;
    /// `Multi_key s s`), so the seam is exercised on the exact values the compositor
    /// scenario drives.
    const COMMIT_CASES: &[CommitCase] = &[
        CommitCase {
            desc: "an empty commit is a routine event and yields no key at all",
            text: "",
            survivors: "",
        },
        CommitCase {
            desc: "the acute-accent sequence GTK's compose table produces",
            text: "é",
            survivors: "é",
        },
        CommitCase {
            desc: "a Multi_key ligature, which is two committed chars in one glyph",
            text: "æ",
            survivors: "æ",
        },
        CommitCase {
            desc: "a Multi_key sharp s",
            text: "ß",
            survivors: "ß",
        },
        CommitCase {
            desc: "a MULTI-CHARACTER commit, in source order — what an engine sends",
            text: "the",
            survivors: "the",
        },
        CommitCase {
            desc: "a control character between two printables is dropped, they are not",
            text: "a\u{1b}b",
            survivors: "ab",
        },
        CommitCase {
            desc: "a bare newline yields nothing: Key::Enter is gdk_key's to produce",
            text: "\n",
            survivors: "",
        },
        CommitCase {
            desc: "a bare tab likewise: Key::Tab has its own route",
            text: "\t",
            survivors: "",
        },
        CommitCase {
            desc: "a bare ESC likewise, and this is the sharpest of the four",
            text: "\u{1b}",
            survivors: "",
        },
        CommitCase {
            desc: "DEL is a control character too, though it is not in the C0 block",
            text: "\u{7f}",
            survivors: "",
        },
        CommitCase {
            desc: "`<` survives as itself — the escaping is key_to_notation's job",
            text: "<",
            survivors: "<",
        },
    ];

    /// How many named fixtures the commit corpus carries. A literal, for
    /// [`ROUTE_CASES`]'s reason: `COMMIT_CASES.len()` compared against itself would
    /// hold for any number of rows, including none.
    const NAMED_COMMIT_CASES: usize = 11;

    /// The events a fixture's `survivors` string stands for.
    ///
    /// Written out here rather than by calling [`commit_keys`], which would compare
    /// the implementation with itself. The four `false`s are the rule
    /// [`no_committed_key_carries_a_modifier`] states by name.
    fn expected_events(survivors: &str) -> Vec<KeyEvent> {
        survivors
            .chars()
            .map(|c| KeyEvent {
                key: Key::Char(c),
                ctrl: false,
                shift: false,
                alt: false,
                meta: false,
            })
            .collect()
    }

    /// Pins what one committed string becomes: which characters survive, and in
    /// what order.
    ///
    /// The four control-only rows and the mixed `a`-ESC-`b` row are what the
    /// `is_control` filter is measured by; the `"the"` row is what a
    /// `text.chars().rev()` or a `text.chars().next()` is measured by. Neither
    /// class can stand in for the other.
    #[test]
    fn commit_keys_table() {
        assert_eq!(
            COMMIT_CASES.len(),
            NAMED_COMMIT_CASES,
            "the commit corpus must carry the {NAMED_COMMIT_CASES} named fixtures, but carries {}. \
             The rows are not a sample: four of them are control-only, one mixes a control \
             character with printables, one is multi-character and three are composed glyphs, so \
             dropping any one leaves a whole class of the rule unmeasured — and three later tests \
             walk this same corpus.",
            COMMIT_CASES.len()
        );

        for case in COMMIT_CASES {
            let got = commit_keys(case.text);
            assert_eq!(
                got,
                expected_events(case.survivors),
                "commit_keys({:?}) must yield one modifier-free Key::Char per surviving character \
                 of {:?}, in that order, got {got:?} — {}. Order is the user's typing order and \
                 nothing downstream restores it: `apply_batch` fuses a run of consecutive Key \
                 events into ONE `nvim_input` call, so a reversed or truncated vec reaches Neovim \
                 as reversed or truncated text.",
                case.text,
                case.survivors,
                case.desc
            );
        }
    }

    /// No committed key carries a modifier, over the whole corpus.
    ///
    /// A commit is a RESULT, not a chord: the modifiers that produced it were spent
    /// inside the compose sequence or inside the engine. `shift` is the
    /// shift-glyph contract [`crate::input`] owns — the text already carries the
    /// glyph — and `ctrl`, `alt` and `meta` matter more: `wolf_core::key_to_notation`
    /// turns any of them into a `<C-…>` / `<A-…>` / `<D-…>` chord, so text composed
    /// over `AltGr` would reach Neovim as commands rather than as characters.
    #[test]
    fn no_committed_key_carries_a_modifier() {
        let mut probed = 0_usize;

        for case in COMMIT_CASES {
            for ev in commit_keys(case.text) {
                probed += 1;
                assert!(
                    !ev.ctrl && !ev.shift && !ev.alt && !ev.meta,
                    "commit_keys({:?}) emitted {ev:?}, which carries a modifier — {}. Every \
                     committed key must set all four flags false. `shift` is the shift-glyph \
                     contract `wolf_core::KeyEvent` documents (the char already holds the shifted \
                     glyph, so `S-` is not merely redundant but ignored); `ctrl`, `alt` and `meta` \
                     are worse, because `key_to_notation` renders each as a chord and Neovim runs \
                     a chord as a command.",
                    case.text,
                    case.desc
                );
            }
        }

        assert!(
            probed > 0,
            "the corpus produced NO events at all, so the modifier rule held vacuously. A \
             `commit_keys` returning an empty vec for every input would pass this test while \
             making every composed character — and, since the input method filters plain \
             printables, every ordinary keystroke — unreachable."
        );
    }

    /// Every committed key renders in Neovim notation as the character itself.
    ///
    /// The executable form of the module docs' "a control character must never
    /// become a `Key::Char`". Note which half discriminates: with the `is_control`
    /// filter deleted, `"\u{1b}"` produces an event whose notation IS `"\u{1b}"`, so
    /// the equality below still holds — it is the control-character and
    /// angle-bracket assertions that fire, which is why both are here.
    #[test]
    fn a_committed_key_renders_as_itself_in_neovim_notation() {
        let mut probed = 0_usize;

        for case in COMMIT_CASES {
            for ev in commit_keys(case.text) {
                probed += 1;
                let Key::Char(c) = ev.key else {
                    panic!(
                        "commit_keys({:?}) emitted {ev:?}, whose key is not a Key::Char — {}. A \
                         commit is text, and the named keys have their own route through \
                         `gdk_key`'s named-key table. Manufacturing one here would be a second, \
                         unnamed way to send `<CR>`, `<Esc>` or `<Tab>`, with no modifier handling \
                         and no `<lt>` escaping.",
                        case.text, case.desc
                    )
                };
                let notation = key_to_notation(&ev);
                let expected = if c == '<' {
                    "<lt>".to_string()
                } else {
                    c.to_string()
                };

                assert_eq!(
                    notation, expected,
                    "key_to_notation for the event commit_keys({:?}) produced from {c:?} is \
                     {notation:?}, not {expected:?}. A modifier-free printable must reach \
                     `nvim_input` as itself, with the single exception of `<`, which is special in \
                     Neovim notation and is escaped to `<lt>` by core.",
                    case.text
                );
                assert!(
                    !notation.chars().any(char::is_control),
                    "key_to_notation for the event commit_keys({:?}) produced is {notation:?}, \
                     which contains a CONTROL character. This is the assertion the `is_control` \
                     filter is measured by, and it is the one that fires when the filter is \
                     deleted (the equality above does not: a raw ESC renders as itself). A control \
                     byte written straight into `nvim_input` is not text — `\\u{{1b}}` leaves \
                     insert mode, `\\r` is Enter and `\\t` is Tab — so the next committed character \
                     is executed as a normal-mode command.",
                    case.text
                );
                assert!(
                    !notation.starts_with('<') || notation == "<lt>",
                    "key_to_notation for the event commit_keys({:?}) produced is {notation:?}, an \
                     angle-bracket NAMED key. Committed text must never become one: `gdk_key`'s \
                     named-key table is the single route to `<CR>`, `<Esc>` and `<Tab>`, and two \
                     entry points that disagree about which keys are named is the bug this seam's \
                     control-character rule exists to prevent. `<lt>` is the one permitted form, \
                     and it is core's escaping of a literal `<`.",
                    case.text
                );
            }
        }

        assert!(
            probed > 0,
            "the corpus produced NO events at all, so every assertion above held vacuously — see \
             `no_committed_key_carries_a_modifier`'s message for what that would mean."
        );
    }

    // =========================================================================
    // commit_decision
    // =========================================================================

    /// How many inputs the decision is pinned over: the eleven corpus fixtures
    /// times the two overlay states. Literals, for [`ROUTE_CASES`]'s reason.
    const COMMIT_DECISION_CASES: usize = 11 * 2;

    /// Pins [`commit_decision`] over `(overlay_open, text)`.
    ///
    /// The overlay half is what a "simplification" that drops the parameter would
    /// break, and it is unreachable from this front-end's own glue (GTK's built-in
    /// context commits synchronously inside `filter_keypress`, which is called only
    /// with no overlay open). An out-of-process engine commits on a bus reply at an
    /// arbitrary main-loop turn, and that is the arrival the guard is for.
    #[test]
    fn commit_decision_table() {
        let mut probed = 0_usize;
        let mut sent = 0_usize;
        let mut refused = 0_usize;

        for case in COMMIT_CASES {
            probed += 1;
            let open = commit_decision(true, case.text);
            assert!(
                matches!(open, CommitDecision::Refuse(_)),
                "commit_decision(true, {:?}) answered {open:?} rather than a refusal — {}. A \
                 commit that arrives while the picker, palette, buffer picker or project search is \
                 open belongs to that overlay's own entry, and forwarding it types the user's \
                 query into the buffer behind it. The refusal holds for EVERY text, the empty \
                 string included.",
                case.text,
                case.desc
            );
            refused += 1;

            probed += 1;
            let closed = commit_decision(false, case.text);
            sent += 1;
            assert_eq!(
                closed,
                CommitDecision::Send(expected_events(case.survivors)),
                "commit_decision(false, {:?}) answered {closed:?}, but with no overlay open it \
                 must Send exactly the events {:?} stands for — {}. Sending something else means \
                 the decision is applying a second rule of its own on top of `commit_keys`; \
                 refusing means ordinary typing is being dropped, and since the input method \
                 filters plain printables that is ALL typing, not only accents.",
                case.text,
                case.survivors,
                case.desc
            );
        }

        assert_eq!(
            probed,
            COMMIT_DECISION_CASES,
            "the decision sweep visited {probed} inputs, not the {COMMIT_DECISION_CASES} it claims \
             ({} corpus fixture(s) x 2 overlay states).",
            COMMIT_CASES.len()
        );
        assert!(
            sent > 0 && refused > 0,
            "the sweep produced {sent} Send(s) and {refused} Refuse(s); both must be non-zero or \
             one of the two arms is unmeasured. An implementation that answered the same way \
             whatever `overlay_open` said would show up here as one of these being zero."
        );
    }

    /// An empty commit is a routine event, not a hazard.
    ///
    /// A refusal exists to be PRINTED, so making one out of an empty or
    /// control-only commit puts a diagnostic line in front of a reader for
    /// something harmless. The overlay case below is the discriminating negative:
    /// "empty" is not what refuses, "an overlay is open" is.
    #[test]
    fn an_empty_commit_is_not_a_refusal() {
        assert_eq!(
            commit_decision(false, ""),
            CommitDecision::Send(Vec::new()),
            "commit_decision(false, \"\") must Send an EMPTY vec: nothing is forwarded and \
             nothing is printed. A refusal here would print a reason on an event that carries no \
             hazard at all — `gtk_im_context_reset` and an engine's own bookkeeping both produce \
             empty commits — and would train a reader to ignore the line that matters."
        );
        assert_eq!(
            commit_decision(false, "\u{1b}"),
            CommitDecision::Send(Vec::new()),
            "a control-only commit must Send an empty vec for the same reason an empty one does: \
             the characters were dropped by `commit_keys`, which is a routine outcome rather than \
             a refusal. This row is what stops the two being told apart by the wrong rule."
        );

        // The discriminating negative: what refuses is the overlay, not the
        // emptiness. Without it, a `commit_decision` that refused on `overlay_open`
        // OR on empty text would satisfy the pair above.
        let behind_an_overlay = commit_decision(true, "");
        assert!(
            matches!(behind_an_overlay, CommitDecision::Refuse(_)),
            "commit_decision(true, \"\") answered {behind_an_overlay:?} rather than a refusal. The \
             overlay guard holds for EVERY text including the empty string, and this assertion is \
             what keeps the two rules above from being read as \"empty text decides\"."
        );
    }

    /// Whether `reason` opens like a SENTENCE (`Xy…`) rather than as a mid-sentence
    /// clause. Re-declared here rather than imported from
    /// [`crate::nav_ui`]'s test module — the file's own precedent — including its
    /// deliberate allowance for an ALL-CAPS opener such as `GNOME`.
    fn is_sentence_cased(reason: &str) -> bool {
        let mut chars = reason.chars();
        let (Some(first), Some(second)) = (chars.next(), chars.next()) else {
            return false;
        };
        first.is_uppercase() && second.is_lowercase()
    }

    /// The refusal reason is non-empty and reads as a mid-sentence clause.
    ///
    /// It is interpolated into a printed diagnostic exactly as
    /// [`crate::nav_ui`]'s and [`crate::command_action`]'s reasons are, and it is
    /// the whole of what a reader sees when a keystroke appears to have vanished —
    /// so an empty one makes a deliberate refusal indistinguishable from a commit
    /// that was silently dropped.
    #[test]
    fn the_refusal_reason_is_non_empty_and_reads_as_a_clause() {
        let mut probed = 0_usize;

        for case in COMMIT_CASES {
            let decision = commit_decision(true, case.text);
            let reason = match &decision {
                CommitDecision::Refuse(reason) => *reason,
                CommitDecision::Send(_) => panic!(
                    "commit_decision(true, {:?}) produced {decision:?} rather than a refusal, so \
                     this test cannot check the reason it exists to check. `commit_decision_table` \
                     is red alongside it, and that is where the finding is.",
                    case.text
                ),
            };
            probed += 1;

            assert!(
                !reason.trim().is_empty(),
                "the refusal reason for the commit {:?} is empty. It is printed as \
                 \"wolf-gnome: commit not forwarded: {{reason}}\", so an empty one leaves a line \
                 that says a commit was dropped and nothing about why.",
                case.text
            );
            assert!(
                !is_sentence_cased(reason),
                "the refusal reason for the commit {:?} is sentence-cased ({reason:?}). It is \
                 interpolated MID-SENTENCE into the printed diagnostic, exactly as \
                 `command_action.rs`'s and `nav_ui.rs`'s reasons are, so it must start lowercase \
                 (an ALL-CAPS proper noun such as \"GNOME\" is fine).",
                case.text
            );
        }

        assert_eq!(
            probed,
            COMMIT_CASES.len(),
            "only {probed} of the {} corpus fixtures reached a refusal, so the rules above were \
             stated over less than the whole corpus.",
            COMMIT_CASES.len()
        );
    }
}
