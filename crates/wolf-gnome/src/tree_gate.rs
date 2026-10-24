//! The pure, GTK-free gate that decides whether a sidebar refresh is worth doing.
//!
//! A deliberate port of `WolfKeyMap/TreeRefreshGate.swift`, holding both halves the
//! macOS counterpart holds:
//!
//! - **Visibility.** A refresh requested while the sidebar is hidden is refused and
//!   REMEMBERED; the sidebar reappearing owes exactly one catch-up. Rebuilding a
//!   widget tree nobody can see is redundant work, and dropping the request without
//!   remembering it would leave a re-shown sidebar holding whatever it last drew.
//! - **Version.** A rebuild happens on the FIRST refresh at a snapshot version and
//!   on no later one. That is what stops a current-file change — which advances no
//!   version — collapsing every manually expanded directory, because in
//!   `wolf-gnome` a rebuild of the root store destroys each row's `TreeListRow` and
//!   with it its expanded state.
//!
//! [`TreeRefreshGate::reset_version_tracking`] exists because
//! `wolf_core::files::FileSearchState` numbers versions PER `FileSearch`, not
//! globally: a fresh search started by a re-root begins again at 1, so a gate that
//! survived the re-root with its tracking intact would answer "no rebuild" to the
//! new project's first batch and leave the previous project's files on screen.

/// The sidebar's refresh gate: visibility plus snapshot version.
///
/// Not `Copy` and deliberately mutable — every method records something. Lives
/// behind the sidebar's `RefCell` and is consulted twice per refresh: the
/// visibility half first, the version half second.
pub struct TreeRefreshGate {
    /// Whether the sidebar is on screen.
    visible: bool,
    /// Whether a refresh was refused while hidden, and is therefore owed.
    deferred: bool,
    /// The snapshot version the widgets were last rebuilt from.
    last_rebuilt_version: Option<u64>,
}

impl TreeRefreshGate {
    /// A gate for a sidebar that is currently `visible`, with nothing deferred and
    /// nothing yet rebuilt.
    #[must_use]
    pub fn new(visible: bool) -> TreeRefreshGate {
        TreeRefreshGate {
            visible,
            deferred: false,
            last_rebuilt_version: None,
        }
    }

    /// Whether a requested refresh may proceed. While hidden it records that one
    /// was owed and answers `false`.
    pub fn request_refresh(&mut self) -> bool {
        if self.visible {
            return true;
        }
        self.deferred = true;
        false
    }

    /// Records the sidebar's visibility, answering `true` iff this is a
    /// hidden → visible transition that owes the one deferred catch-up (which it
    /// then clears).
    pub fn set_visible(&mut self, visible: bool) -> bool {
        let was_visible = self.visible;
        self.visible = visible;
        if !visible || was_visible || !self.deferred {
            return false;
        }
        self.deferred = false;
        true
    }

    /// Whether the widgets must be rebuilt for `version`: `true` iff it differs
    /// from the version last rebuilt, which it then records.
    pub fn should_rebuild(&mut self, version: u64) -> bool {
        if self.last_rebuilt_version == Some(version) {
            return false;
        }
        self.last_rebuilt_version = Some(version);
        true
    }

    /// Forgets the last rebuilt version.
    ///
    /// Called on a RE-ROOT: a fresh `wolf_core::FileSearch` restarts its version at
    /// 1, so the new project's first batch would otherwise be indistinguishable
    /// from a redundant wake at a version already drawn.
    pub fn reset_version_tracking(&mut self) {
        self.last_rebuilt_version = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === The version gate ===

    /// One call in a version-gate SEQUENCE: the snapshot version the caller passes
    /// and whether the gate must answer "rebuild".
    ///
    /// Order is the rule here, so the rows are consumed in order against a single
    /// mutable gate rather than each being independent.
    struct VersionCall {
        desc: &'static str,
        version: u64,
        expected: bool,
    }

    /// A rebuild happens on the FIRST call at a version and on no later one, until
    /// the version advances.
    ///
    /// What each failure means: a `true` where `false` was due is a rebuild that
    /// changes nothing on screen and collapses every manually expanded directory
    /// (the whole reason the gate exists); a `false` where `true` was due means a
    /// streamed batch never reaches the widgets at all.
    ///
    /// Reverts that must redden it: making `should_rebuild` always return `true`
    /// fails calls 2, 4 and 5; recording nothing in `last_rebuilt_version` (so the
    /// comparison is against a value that never advances) fails call 2.
    #[test]
    fn version_gate_rebuilds_once_per_version() {
        // Visible, so the visibility half of the gate never masks the version half
        // — this test is about the version rule alone.
        let mut gate = TreeRefreshGate::new(true);

        let calls = [
            VersionCall {
                desc: "the first batch of a session must rebuild: nothing has been drawn yet",
                version: 1,
                expected: true,
            },
            VersionCall {
                desc: "a second wake at the SAME version has nothing new to show",
                version: 1,
                expected: false,
            },
            VersionCall {
                desc: "the version advances: a batch has landed, so rebuild",
                version: 2,
                expected: true,
            },
            VersionCall {
                desc: "a redundant wake at version 2 (e.g. a current-file change)",
                version: 2,
                expected: false,
            },
            VersionCall {
                desc: "and another: repeated same-version wakes stay suppressed indefinitely",
                version: 2,
                expected: false,
            },
            VersionCall {
                desc: "the next advance rebuilds again",
                version: 3,
                expected: true,
            },
        ];

        for (index, call) in calls.iter().enumerate() {
            let got = gate.should_rebuild(call.version);
            assert_eq!(
                got,
                call.expected,
                "call {} of the sequence: should_rebuild({}) must be {} but was {} — {}",
                index + 1,
                call.version,
                call.expected,
                got,
                call.desc
            );
        }
    }

    /// A re-root forgets the last rebuilt version, so the SAME version number
    /// coming from a different search still rebuilds.
    ///
    /// This is the falsifier for the stale-index hazard: `FileSearchState` numbers
    /// versions per `FileSearch`, not globally, so a fresh search after a re-root
    /// starts again at 1. A gate that survived the re-root with its tracking
    /// intact would answer "no rebuild" to the new project's first batch and leave
    /// the previous project's files on screen.
    ///
    /// Revert that must redden it: emptying `reset_version_tracking`'s body.
    #[test]
    fn version_gate_resets_on_reroot() {
        let mut gate = TreeRefreshGate::new(true);

        assert!(
            gate.should_rebuild(2),
            "the first call at version 2 must rebuild: the gate has recorded nothing yet"
        );
        assert!(
            !gate.should_rebuild(2),
            "a repeated call at version 2 must not rebuild — without this the reset below would \
             be unobservable, because every call would rebuild anyway"
        );

        gate.reset_version_tracking();

        assert!(
            gate.should_rebuild(2),
            "after reset_version_tracking the SAME version must rebuild again: a re-root replaces \
             the search, so version 2 of the new search shares nothing but its number with \
             version 2 of the old one"
        );

        // The production shape: version 1, a re-root, and a fresh search starting at version 1 again;
        // without the reset the new project's first batch looks like a redundant wake.
        let mut gate = TreeRefreshGate::new(true);
        assert!(
            gate.should_rebuild(1),
            "the first batch of the FIRST search (version 1) must rebuild"
        );
        gate.reset_version_tracking();
        assert!(
            gate.should_rebuild(1),
            "the first batch of the search started by a re-root is also version 1, and must \
             rebuild — answering false here shows the previous project's files under the new \
             project's root"
        );
    }

    // === The visibility gate ===

    /// While hidden, a refresh is dropped and remembered; the sidebar reappearing
    /// owes exactly ONE catch-up.
    ///
    /// Reverts that must redden it: making `request_refresh` always return `true`
    /// fails at step 1 below (and would fail step 6 too); never recording the
    /// deferral when hidden fails at step 2 (and would fail step 7 too).
    #[test]
    fn visibility_gate_defers_and_catches_up_once() {
        // --- built hidden: the empty state, or a sidebar toggled off ----------
        let mut hidden = TreeRefreshGate::new(false);
        assert!(
            !hidden.request_refresh(),
            "step 1: a refresh requested while the sidebar is HIDDEN must be refused — rebuilding \
             a widget tree nobody can see is the redundant work this gate removes"
        );
        assert!(
            hidden.set_visible(true),
            "step 2: showing the sidebar with a refresh deferred must owe exactly one catch-up, \
             or the sidebar reappears holding whatever it last drew"
        );
        assert!(
            !hidden.set_visible(true),
            "step 3: a second set_visible(true) is not a hidden->visible transition and owes \
             nothing; a second catch-up here would rebuild on every repeated visibility relay"
        );

        // --- built visible: the ordinary project launch ------------------------
        let mut visible = TreeRefreshGate::new(true);
        assert!(
            visible.request_refresh(),
            "step 4: a refresh requested while the sidebar is VISIBLE must proceed immediately"
        );
        assert!(
            !visible.set_visible(false),
            "step 5: HIDING the sidebar owes no catch-up — the catch-up is owed on the way back"
        );
        assert!(
            !visible.request_refresh(),
            "step 6: a refresh requested after hiding must be refused, exactly as in step 1"
        );
        assert!(
            visible.set_visible(true),
            "step 7: re-showing the sidebar after a refused request owes the one catch-up"
        );

        // --- nothing was dropped, so nothing is owed --------------------------
        let mut untouched = TreeRefreshGate::new(false);
        assert!(
            !untouched.set_visible(true),
            "step 8: showing a sidebar that never refused a refresh owes NO catch-up. Without \
             this row, a gate that answered true on every hidden->visible transition would still \
             pass steps 1-7."
        );
    }

    // === The two gates against a streaming traversal ===

    /// Nine wakes carrying three distinct versions produce three rebuilds.
    ///
    /// The two numbers mean different things, which is what makes the assertion a
    /// statement rather than a restatement of the input:
    ///
    /// - 9 is how many WAKES the drain task delivers during one streaming
    ///   traversal — one per batch, plus the redundant wakes that arrive at an
    ///   unchanged version.
    /// - 3 is how many distinct SNAPSHOT VERSIONS those wakes carry, i.e. how many
    ///   rebuilds the sidebar actually owes.
    ///
    /// The expected value below is the literal 3. It is deliberately not
    /// `versions.len()` and not any count derived from `versions`: a count derived
    /// from the input array would also be satisfied by a gate that rebuilt on
    /// every wake, which is precisely the regression this test exists to catch.
    ///
    /// Revert that must redden it: making `should_rebuild` always return `true`,
    /// which yields 9 rebuilds.
    #[test]
    fn nine_stream_events_rebuild_three_times() {
        let versions: [u64; 9] = [1, 1, 1, 2, 2, 3, 3, 3, 3];

        let mut gate = TreeRefreshGate::new(true);
        let mut rebuilds = 0_u32;
        for version in versions {
            if gate.should_rebuild(version) {
                rebuilds += 1;
            }
        }

        assert_eq!(
            rebuilds, 3,
            "nine streaming wakes carrying three distinct snapshot versions must produce exactly \
             3 rebuilds, but produced {rebuilds}. A count of 9 means the gate rebuilds on every \
             wake and the version half is doing nothing; a count below 3 means a version advance \
             was swallowed and a streamed batch never reached the widgets."
        );
    }

    /// A traversal batch that advances the version while the sidebar is HIDDEN must
    /// produce a rebuild on the catch-up: the deferral remembers that work is owed,
    /// and the version gate must then agree that work is owed.
    ///
    /// This drives the composition `TreeInner::refresh` actually performs — the
    /// visibility half first, the version half second — which is the one interaction
    /// neither half proves alone. `visibility_gate_defers_and_catches_up_once` pins
    /// the visibility half by itself and never calls `should_rebuild` at all, so a
    /// gate that owed the catch-up and then refused to rebuild on it passes that
    /// test; `version_gate_rebuilds_once_per_version` never hides the sidebar, so a
    /// gate that forgot the deferral passes that one. The two are therefore not
    /// duplicates of this: what is added here is the JOIN, a catch-up whose version
    /// advanced during the hidden window and which the widgets have never drawn.
    ///
    /// The converse is deliberately NOT asserted as a rebuild. Hiding the sidebar is
    /// `sidebar.set_visible(false)`, which destroys neither the `gio::ListStore` nor
    /// the `gtk::TreeListModel`, so the widgets still hold whatever version they last
    /// drew and a catch-up at that same version correctly answers "no rebuild" rather
    /// than leaving anything stale. Step 6 pins that answer.
    ///
    /// Reverts that must redden it, with the step each hits:
    ///
    /// - Deleting `self.deferred = true;` from `request_refresh` fails STEP 4. This
    ///   is the failure the test exists to catch: the batch that landed while hidden
    ///   is forgotten, no catch-up is owed, and the re-shown sidebar is missing every
    ///   file that streamed in. It also reddens step 2 of
    ///   `visibility_gate_defers_and_catches_up_once`, which shares that half.
    /// - Answering `false` from `should_rebuild` whenever anything has already been
    ///   drawn (`if self.last_rebuilt_version.is_some() { return false; }`) fails
    ///   STEP 5 — the assertion that names the invariant — while leaving steps 1-4
    ///   green.
    /// - Making `request_refresh` always return `true` fails STEP 3.
    /// - Making `should_rebuild` always return `true` fails STEP 6.
    #[test]
    fn a_batch_arriving_while_hidden_rebuilds_on_the_catch_up() {
        // --- the launch: sidebar on screen, first batch drawn -----------------
        let mut gate = TreeRefreshGate::new(true);
        assert!(
            gate.request_refresh(),
            "step 1a: a refresh requested at launch, with the sidebar VISIBLE, must proceed — \
             refused here, the sidebar never draws its first batch at all"
        );
        assert!(
            gate.should_rebuild(1),
            "step 1b: version 1 is the first batch of the session and must rebuild; without this \
             the launch leaves an empty tree, and the hidden window below would start from a gate \
             that had drawn nothing"
        );

        // --- the user hides the sidebar --------------------------------------
        assert!(
            !gate.set_visible(false),
            "step 2: HIDING the sidebar owes no catch-up — nothing has been refused yet, and a \
             catch-up owed on the way out would rebuild a tree nobody can see"
        );

        // --- A batch lands at version 2 while hidden: refused before the search view is read, so the
        // gate still tracks 1 and step 5's catch-up is the only call that ever sees version 2.
        assert!(
            !gate.request_refresh(),
            "step 3: a traversal batch arriving while the sidebar is HIDDEN must be refused, and \
             recorded as owed; rebuilding an off-screen widget tree is the redundant work this \
             gate removes"
        );

        // --- the user shows it again ------------------------------------------
        assert!(
            gate.set_visible(true),
            "step 4: re-showing the sidebar after a refused batch must owe exactly one catch-up. \
             A false here means the deferral was dropped, so no refresh is ever attempted and the \
             version advance below is never even offered to the version gate."
        );

        // --- the catch-up: the payload of this test ---------------------------
        assert!(
            gate.should_rebuild(2),
            "step 5: the catch-up refresh carries version 2, which the widgets have never drawn, \
             so it MUST rebuild. A false here is the composed failure neither gate shows alone: \
             the sidebar re-appears missing every file that streamed in while it was hidden, and \
             stays wrong until the next batch happens to advance the version again."
        );
        assert!(
            !gate.should_rebuild(2),
            "step 6: the catch-up is ONE rebuild, not a licence to rebuild on every later wake. A \
             true here means a redundant wake at an unchanged version rebuilds the root store, \
             collapsing every directory the user had manually expanded."
        );
    }
}
