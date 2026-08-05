//! The rulebook for the OS DARK/LIGHT APPEARANCE: what a configured `appearance`
//! resolves to against libadwaita's own reading of the desktop, plus the few lines
//! of GTK glue that carry the answer out.
//!
//! [`appearance_decision`] is pure — values in, values out — and
//! [`apply_at_launch`] is the applier that delivers its answer to the two places
//! it has to reach: the captive Neovim's `background` option, through
//! `WolfCore::set_background`, and the GTK chrome, through
//! `adw::StyleManager::set_color_scheme`. That split is [`crate::about`]'s shape
//! and it is chosen for the same reason: the decision is worth testing and the
//! handful of `adw` calls that act on it are not.
//!
//! ## Why this is not an addition to `config_seam`
//!
//! [`crate::config_seam`] is documented as GTK-free and environment-free, and a
//! function returning an [`adw::ColorScheme`] would falsify that in one line.
//! Splitting the appearance rules into their own file keeps that property true
//! rather than quietly weakening it.
//!
//! ## `os_dark` is INJECTED, never read here
//!
//! `adw::StyleManager::default()` carries `assert_initialized_main_thread!()`, so
//! any code path that reaches for it aborts outright in a headless unit test. The
//! desktop's reading is therefore a plain `bool` PARAMETER: [`apply_at_launch`]
//! makes the one `is_dark()` call, on the main thread where it is legal, and hands
//! the answer in. That is what keeps the whole rulebook executable with no
//! display, no compositor and no GTK — and it is the reason nothing in this module
//! may be "simplified" by reading the style manager inside the decision.
//!
//! Returning an `adw::ColorScheme` from a pure function is the precedent
//! [`crate::actions`] sets for `gdk::Key` and `gdk::ModifierType`: a plain value
//! type with no widget, no display and no initialized main thread behind it, so
//! testability costs nothing and the seam names exactly what the widget layer will
//! call.
//!
//! ## Totality is the contract
//!
//! [`appearance_decision`] matches [`wolf_core::Appearance`] with **no `_` arm**,
//! so a fourth variant added in `wolf-core` is an `E0004` compile error here until
//! it is classified — the same guard [`crate::nav_ui::nav_press`] and
//! [`crate::liveness_ui`] rely on. A wildcard arm would silently resolve a future
//! `Appearance::HighContrast` to whatever the fallback happened to be.
//!
//! ## The applier's step order, which is load-bearing
//!
//! Inside [`apply_at_launch`], in this order:
//!
//! 1. take the `adw::StyleManager`;
//! 2. resolve the decision from the configured value and `manager.is_dark()`;
//! 3. `set_color_scheme(decided.color_scheme)`;
//! 4. push `set_background(decided.dark)` into the core, with the borrow taken and
//!    released in its own statement;
//! 5. only if `decided.observe_os`, connect the `dark` notify handler.
//!
//! Step 5 comes after step 3 on purpose: connecting first would let the scheme
//! change made in step 3 fire the handler's own first run. The call site in
//! [`crate::app`] is likewise placed after the core exists — so the push has
//! somewhere to go — and before any widget is built, so the window is realized
//! under the resolved scheme with no visible flash.
//!
//! ## The notify handler pushes, and deliberately never re-sets the scheme
//!
//! The handler re-runs [`appearance_decision`] against the manager's new reading
//! and calls `set_background` only. It does **not** call `set_color_scheme` again:
//! re-applying a property from inside that property's own notify handler is the
//! shape that loops. No dedup guard is needed either, unlike the macOS
//! counterpart's `guard nowDark != resolvedDark` — `notify::dark` fires only on a
//! real property change, and `set_background` is idempotent and fire-and-forget on
//! the ordered input channel.
//!
//! ## A notify is neither a timer nor a poll
//!
//! `crates/wolf-gnome/DECISIONS.md`, "Threading and the wake", records that
//! nothing in this crate polls and that no timer re-arms while the application is
//! idle. A `StyleManager` `dark` notification is a GObject property notification:
//! armed once at launch, in `auto` mode only, and fired only when libadwaita's own
//! property actually changes. It adds no third one-shot timer and nothing re-arms
//! itself, so that property is intact — stated here so a later reader does not go
//! looking for a timer that does not exist.

use std::cell::RefCell;
use std::rc::Rc;

use wolf_core::{Appearance, WolfConfig, WolfCore};

/// The resolved appearance: what to push into Neovim, what to set on the GTK
/// chrome, and whether to follow a live OS change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedAppearance {
    /// What `WolfCore::set_background` is pushed: `true` selects
    /// `background=dark`, `false` selects `background=light`.
    pub dark: bool,
    /// What `adw::StyleManager::set_color_scheme` is given for the app chrome.
    pub color_scheme: adw::ColorScheme,
    /// Whether to arm the `dark` notify handler, so a live desktop toggle
    /// re-pushes. The port of macOS's `installAppearanceObserverIfNeeded` /
    /// `removeAppearanceObserver`: only `auto` follows a live toggle.
    pub observe_os: bool,
}

/// Resolves the configured `appearance` against the desktop's current
/// light/dark preference.
///
/// `os_dark` is libadwaita's `StyleManager::is_dark()` reading, passed in rather
/// than read (see the module docs — reading it here would make this function
/// uncallable from a headless test). TOTAL by construction: a `match` over
/// [`Appearance`] with no `_` arm.
#[must_use]
pub fn appearance_decision(configured: Appearance, os_dark: bool) -> ResolvedAppearance {
    todo!(
        "appearance::appearance_decision is not implemented yet (configured: {configured:?}, \
         os_dark: {os_dark})"
    )
}

/// Applies the resolved appearance at launch: sets the chrome's colour scheme,
/// pushes `background` into the captive Neovim, and — in `auto` only — arms the
/// handler that re-pushes when the desktop toggles.
///
/// **Deliberately untested.** It reaches for `adw::StyleManager::default()`,
/// whose `assert_initialized_main_thread!()` aborts without a live GTK main
/// thread, so there is no headless test for this function and none should be
/// added. What is testable is the decision it carries out, which is
/// [`appearance_decision`]'s six-row table; what is observable end to end is that
/// the configured value reaches Neovim's `background`, which
/// `configured_appearance_reaches_neovims_background_under_cage` in
/// `tests/smoke_headless.rs` is what proves. The step order this function must
/// keep is in the module docs.
pub fn apply_at_launch(config: &WolfConfig, core: &Rc<RefCell<Option<WolfCore>>>) {
    let _ = (config, core);
    todo!("appearance::apply_at_launch is not implemented yet")
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // The whole input domain
    // =========================================================================

    /// Every `appearance` a `wolf-config.yaml` can carry.
    const ALL_APPEARANCES: [Appearance; 3] =
        [Appearance::Auto, Appearance::Dark, Appearance::Light];

    /// Every reading `adw::StyleManager::is_dark()` can produce.
    const ALL_OS_READINGS: [bool; 2] = [false, true];

    /// How many rows the decision table carries, written as a LITERAL rather than
    /// as `ALL_APPEARANCES.len() * ALL_OS_READINGS.len()`, and that is the whole
    /// point of it — the `nav_ui::tests::SWEEP_INPUTS` and
    /// `clipboard_ui::tests::NAMED_LIVENESS_INPUTS` precedent. Comparing a table's
    /// length to the length of the arrays the table was built from is a TAUTOLOGY:
    /// an array that lost an entry shrinks both sides together, so the check
    /// passes while the table covers less than the domain it claims to.
    ///
    /// It is also the number that makes the `_`-free match legible from the test
    /// side. Three configured values times two desktop readings is six, and six
    /// rows means every arm is measured; a fourth `Appearance` variant is an
    /// `E0004` in the seam and a red row count here.
    const NAMED_DECISION_ROWS: usize = 6;

    /// One [`appearance_decision`] table row.
    struct DecisionCase {
        desc: &'static str,
        configured: Appearance,
        os_dark: bool,
        expected: ResolvedAppearance,
    }

    /// The whole rulebook, one row per `(configured, os_dark)` pair.
    ///
    /// A `const` item rather than a local so the count assertion can name it and
    /// so the sweeps below can walk the same rows without copying them.
    const DECISION_CASES: &[DecisionCase] = &[
        DecisionCase {
            desc: "auto on a LIGHT desktop: follow it, and leave the chrome to libadwaita",
            configured: Appearance::Auto,
            os_dark: false,
            expected: ResolvedAppearance {
                dark: false,
                color_scheme: adw::ColorScheme::Default,
                observe_os: true,
            },
        },
        DecisionCase {
            desc: "auto on a DARK desktop: follow it, same untouched chrome",
            configured: Appearance::Auto,
            os_dark: true,
            expected: ResolvedAppearance {
                dark: true,
                color_scheme: adw::ColorScheme::Default,
                observe_os: true,
            },
        },
        DecisionCase {
            desc: "a forced dark OVERRIDES a light desktop -- the row a seam that read only \
                   os_dark gets wrong",
            configured: Appearance::Dark,
            os_dark: false,
            expected: ResolvedAppearance {
                dark: true,
                color_scheme: adw::ColorScheme::ForceDark,
                observe_os: false,
            },
        },
        DecisionCase {
            desc: "a forced dark on a dark desktop agrees with it, and still stops observing",
            configured: Appearance::Dark,
            os_dark: true,
            expected: ResolvedAppearance {
                dark: true,
                color_scheme: adw::ColorScheme::ForceDark,
                observe_os: false,
            },
        },
        DecisionCase {
            desc: "a forced light on a light desktop agrees with it, and still stops observing",
            configured: Appearance::Light,
            os_dark: false,
            expected: ResolvedAppearance {
                dark: false,
                color_scheme: adw::ColorScheme::ForceLight,
                observe_os: false,
            },
        },
        DecisionCase {
            desc: "a forced light OVERRIDES a dark desktop -- the other row a seam that read only \
                   os_dark gets wrong",
            configured: Appearance::Light,
            os_dark: true,
            expected: ResolvedAppearance {
                dark: false,
                color_scheme: adw::ColorScheme::ForceLight,
                observe_os: false,
            },
        },
    ];

    // =========================================================================
    // appearance_decision
    // =========================================================================

    /// Pins all three output fields for EVERY `(configured, os_dark)` input.
    ///
    /// ## What it discriminates, and the edits that redden it
    ///
    /// FALSIFYING EDIT: return `dark: true` for `Appearance::Light`. That is the
    /// revert the compositor smoke
    /// `configured_appearance_reaches_neovims_background_under_cage` also catches,
    /// and the pair is a DIAGNOSTIC: a red smoke over a red fast suite means the
    /// rule itself is wrong here, while a red smoke over a green fast suite means
    /// the rule is right and `apply_at_launch` is not carrying it out.
    ///
    /// SECOND FALSIFYING EDIT: give every arm `color_scheme:
    /// adw::ColorScheme::Default`. Rows 3 to 6 go red, and that is exactly the
    /// pre-declared fallback shape — if it is ever taken deliberately, this table
    /// is the file that records it, not an assertion that was weakened.
    ///
    /// The row-count assertion is what makes the table non-vacuous: a row that
    /// quietly went missing cannot hide behind the five that remain, and the rows
    /// most likely to go missing are the two crossed ones (a forced value against
    /// the OPPOSITE desktop reading), which are the only rows a seam that ignored
    /// `configured` and a seam that ignored `os_dark` disagree on.
    #[test]
    fn appearance_decision_table() {
        assert_eq!(
            DECISION_CASES.len(),
            NAMED_DECISION_ROWS,
            "the appearance_decision table must carry exactly one row per input the seam is total \
             over: {NAMED_DECISION_ROWS} (three configured values times two desktop readings), not \
             {}. A missing row means an arm nothing checks.",
            DECISION_CASES.len()
        );
        assert_eq!(
            ALL_APPEARANCES.len() * ALL_OS_READINGS.len(),
            NAMED_DECISION_ROWS,
            "the domain arrays describe {} input(s) but the seam is total over \
             {NAMED_DECISION_ROWS}. If `wolf_core::Appearance` gained a variant, the `_`-free match \
             in this module is an E0004 already and these constants are the second thing to update; \
             if an entry was DELETED, every sweep below silently covers less than the domain it \
             claims to.",
            ALL_APPEARANCES.len() * ALL_OS_READINGS.len()
        );

        for case in DECISION_CASES {
            let got = appearance_decision(case.configured, case.os_dark);
            assert_eq!(
                got, case.expected,
                "appearance_decision({:?}, os_dark: {}) must be {:?}, got {got:?} — {}. A wrong \
                 `dark` pushes the opposite `background` into Neovim, so the editor's own \
                 colorscheme inverts against the user's configured choice; a wrong `color_scheme` \
                 leaves the header bar, the sidebar and the four overlays in the opposite theme \
                 from the grid they surround; a wrong `observe_os` either freezes `auto` at its \
                 launch-time reading or lets a desktop toggle overwrite a value the user forced in \
                 `wolf-config.yaml`.",
                case.configured, case.os_dark, case.expected, case.desc
            );
        }

        // Every named row of the domain really is present, so the count above
        // cannot be satisfied by six copies of one row.
        for configured in ALL_APPEARANCES {
            for os_dark in ALL_OS_READINGS {
                assert!(
                    DECISION_CASES
                        .iter()
                        .any(|case| case.configured == configured && case.os_dark == os_dark),
                    "no appearance_decision table row covers (configured: {configured:?}, os_dark: \
                     {os_dark}), so that pair's answer is unmeasured. The count assertion above \
                     passes on six rows whatever they are; this is what makes them the six."
                );
            }
        }
    }

    /// In `auto` the answer FOLLOWS the desktop, and the chrome is left alone.
    ///
    /// ## What it discriminates
    ///
    /// FALSIFYING EDIT: map `Auto` to `ForceDark` / `ForceLight` according to
    /// `os_dark`. Every `dark` value stays correct, so a table that pinned only
    /// `dark` would stay green — and the app would pin its chrome to whatever the
    /// desktop read at launch, defeating the live following libadwaita already
    /// does for free and putting the shipped default into the "forcing" mode
    /// libadwaita reserves for a user who explicitly asked. `ColorScheme::Default`
    /// IS "follow the desktop", which is why `auto`'s chrome half is a deliberate
    /// no-op rather than an omission.
    ///
    /// SECOND FALSIFYING EDIT: return a constant `dark` for `Auto`. The two rows
    /// collapse, and `auto` stops meaning "follow" at all.
    #[test]
    fn auto_follows_the_os_and_leaves_the_chrome_to_libadwaita() {
        for os_dark in ALL_OS_READINGS {
            let got = appearance_decision(Appearance::Auto, os_dark);
            assert_eq!(
                got.dark, os_dark,
                "appearance_decision(Auto, os_dark: {os_dark}) resolved dark = {}, but `auto` MEANS \
                 follow the desktop: the pushed `background` must equal the reading it was handed. \
                 A constant here makes `auto` — the shipped default, and what every config that \
                 predates the key parses to — ignore the desktop entirely.",
                got.dark
            );
            assert_eq!(
                got.color_scheme,
                adw::ColorScheme::Default,
                "appearance_decision(Auto, os_dark: {os_dark}) asked for {:?} on the GTK chrome. In \
                 `auto` the scheme must be ColorScheme::Default, which IS \"follow the desktop\" — \
                 libadwaita already does that, live, for free. Answering ForceDark or ForceLight \
                 here pins the chrome to whatever the desktop happened to read at launch and stops \
                 it following at all, and it forces a scheme for a user who never asked for one, \
                 which is the one thing libadwaita's own guidance says not to do.",
                got.color_scheme
            );
        }

        // Non-vacuity for the clause above: the two readings really do produce two
        // different answers, so "follows the desktop" is being measured rather
        // than satisfied by a seam that returns one row for everything.
        assert_ne!(
            appearance_decision(Appearance::Auto, false),
            appearance_decision(Appearance::Auto, true),
            "the two `auto` rows produced the SAME answer, so nothing above distinguishes a seam \
             that follows the desktop from one that ignores its `os_dark` argument."
        );
    }

    /// A forced `appearance` overrides the desktop, in BOTH directions, and it
    /// forces the scheme rather than merely preferring it.
    ///
    /// ## What it discriminates
    ///
    /// FALSIFYING EDIT 1: resolve `dark` from `os_dark` in the forced arms. The
    /// crossed rows go red — a user who wrote `appearance: dark` on a light
    /// desktop is the whole point of the setting, and this is the shape that
    /// silently ignores them.
    ///
    /// FALSIFYING EDIT 2: use `adw::ColorScheme::PreferDark` /
    /// `PreferLight`. This is the look-alike that matters, and it is why the
    /// negative is asserted rather than left implied: `Prefer*` yields to the
    /// desktop's own preference, so a forced appearance would be quietly
    /// overridden by exactly the setting the user wrote the config line to
    /// override. `Force*` is the only pair that means what `appearance: dark`
    /// says.
    #[test]
    fn a_forced_appearance_overrides_the_os_in_both_directions() {
        let forced = [
            (
                Appearance::Dark,
                true,
                adw::ColorScheme::ForceDark,
                adw::ColorScheme::PreferDark,
            ),
            (
                Appearance::Light,
                false,
                adw::ColorScheme::ForceLight,
                adw::ColorScheme::PreferLight,
            ),
        ];

        for (configured, want_dark, want_scheme, lookalike) in forced {
            let on_light = appearance_decision(configured, false);
            let on_dark = appearance_decision(configured, true);

            assert_eq!(
                on_light, on_dark,
                "appearance_decision({configured:?}, …) answered {on_light:?} on a light desktop \
                 and {on_dark:?} on a dark one. A FORCED appearance must not depend on the desktop \
                 reading at all — that is what \"forced\" means, and it is the only reason the \
                 setting exists: someone whose desktop is light and who wants Wolf dark."
            );

            for (reading, got) in [("light", on_light), ("dark", on_dark)] {
                assert_eq!(
                    got.dark, want_dark,
                    "appearance_decision({configured:?}, on a {reading} desktop) resolved dark = \
                     {}, want {want_dark}. This is the value pushed as `:set background=…`, so \
                     getting it wrong inverts the captive Neovim's own colorscheme against the \
                     user's configured choice.",
                    got.dark
                );
                assert_eq!(
                    got.color_scheme, want_scheme,
                    "appearance_decision({configured:?}, on a {reading} desktop) asked for {:?} on \
                     the GTK chrome, want {want_scheme:?}.",
                    got.color_scheme
                );
                assert_ne!(
                    got.color_scheme, lookalike,
                    "appearance_decision({configured:?}, on a {reading} desktop) asked for \
                     {lookalike:?}. That is the LOOK-ALIKE, not the answer: a `Prefer…` scheme \
                     yields to the desktop's own preference, so a user who forced an appearance \
                     would have it silently overridden by exactly the system setting they wrote \
                     the config line to override. Only {want_scheme:?} forces."
                );
                assert_ne!(
                    got.color_scheme,
                    adw::ColorScheme::Default,
                    "appearance_decision({configured:?}, on a {reading} desktop) left the chrome at \
                     ColorScheme::Default, which means \"follow the desktop\". The grid would then \
                     honour the forced appearance while the header bar, sidebar and overlays around \
                     it followed the desktop — the two halves of one window disagreeing."
                );
            }
        }
    }

    /// Only `auto` arms the live OS observer.
    ///
    /// ## What it discriminates
    ///
    /// FALSIFYING EDIT: `observe_os: true` in every arm. The handler is then armed
    /// with a forced appearance in effect, so a desktop toggle re-pushes a
    /// `background` that contradicts the user's own config line — the exact reason
    /// the macOS counterpart REMOVES its observer in the forced arms rather than
    /// leaving it installed.
    ///
    /// COMPLEMENTARY EDIT: `observe_os: false` in every arm. `auto` then becomes a
    /// launch-time snapshot: correct until the desktop changes, and stale
    /// afterwards with no way for the user to tell why.
    ///
    /// The whole-domain shape is the one
    /// `nav_ui::only_a_live_core_with_a_real_window_may_move` uses: a failure names
    /// the rule rather than quoting a row index, and a table whose six rows became
    /// six copies of one is caught here rather than by the count.
    #[test]
    fn only_auto_arms_the_os_observer() {
        let mut observing: Vec<(Appearance, bool)> = Vec::new();
        for configured in ALL_APPEARANCES {
            for os_dark in ALL_OS_READINGS {
                if appearance_decision(configured, os_dark).observe_os {
                    observing.push((configured, os_dark));
                }
            }
        }

        let expected = [(Appearance::Auto, false), (Appearance::Auto, true)];
        assert_eq!(
            observing.as_slice(),
            expected.as_slice(),
            "exactly the two `auto` inputs may arm the live OS observer, whatever the desktop \
             currently reads. An EXTRA entry means the `dark` notify handler stays connected while \
             a FORCED appearance is in effect, so the next desktop toggle re-pushes a `background` \
             that contradicts the user's own `wolf-config.yaml` line — the state macOS avoids by \
             removing its observer in the forced arms. An entry MISSING means `auto` is resolved \
             once at launch and never again, which is a snapshot rather than the following the \
             setting's name promises."
        );
    }

    /// The chrome's scheme and the editor's `background` never disagree.
    ///
    /// ## What it discriminates
    ///
    /// This is a CROSS-FIELD invariant, and no row of the table states it: each
    /// row pins two values independently, so a single-field edit that swapped one
    /// of them reads as one wrong row rather than as the user-visible defect,
    /// which is a window whose chrome and whose grid are in opposite themes. The
    /// rule stated here is the one a reader can check by looking at a screenshot.
    ///
    /// FALSIFYING EDIT: pair `ColorScheme::ForceDark` with `dark: false` (or the
    /// mirror). The message names the outcome rather than the row.
    ///
    /// The `Default` arm carries the same rule in the only form it can take there:
    /// the chrome follows the desktop, so the pushed `background` must be the
    /// desktop's reading too.
    #[test]
    fn the_chrome_scheme_and_the_editor_background_never_disagree() {
        let mut saw_dark = false;
        let mut saw_light = false;

        for configured in ALL_APPEARANCES {
            for os_dark in ALL_OS_READINGS {
                let got = appearance_decision(configured, os_dark);
                if got.dark {
                    saw_dark = true;
                } else {
                    saw_light = true;
                }

                // `adw::ColorScheme` is `#[non_exhaustive]`, so this match needs a
                // wildcard; the wildcard is a FAILURE arm, because the three
                // schemes below are the only ones this seam may ever produce.
                let want_dark = match got.color_scheme {
                    adw::ColorScheme::Default => os_dark,
                    adw::ColorScheme::ForceDark => true,
                    adw::ColorScheme::ForceLight => false,
                    other => panic!(
                        "appearance_decision({configured:?}, os_dark: {os_dark}) asked for \
                         {other:?} on the GTK chrome. This seam may produce only Default (follow \
                         the desktop), ForceDark or ForceLight: a `Prefer…` scheme yields to the \
                         desktop and so does not force what the user forced, and `__Unknown` is \
                         libadwaita's catch-all for a value it did not recognise."
                    ),
                };

                assert_eq!(
                    got.dark, want_dark,
                    "appearance_decision({configured:?}, os_dark: {os_dark}) asked for {:?} on the \
                     GTK chrome while pushing background={} into Neovim. Those disagree: the \
                     header bar, the sidebar rows and the four overlays would be drawn in one \
                     theme and the editor grid they surround in the other, in the same window. The \
                     two fields are resolved from the same input and must never be edited apart.",
                    got.color_scheme,
                    if got.dark { "dark" } else { "light" }
                );
            }
        }

        assert!(
            saw_dark && saw_light,
            "the sweep never produced both a dark and a light answer (saw dark: {saw_dark}, saw \
             light: {saw_light}), so the agreement asserted above held over a single outcome and \
             proves nothing. A seam returning one constant row for every input would satisfy every \
             iteration of this loop."
        );
    }
}
