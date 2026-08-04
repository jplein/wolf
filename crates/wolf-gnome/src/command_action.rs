//! The pure, GTK-free mapping from a drained [`wolf_core::Command`] to the UI
//! action `wolf-gnome` performs for it.
//!
//! `wolf-core` queues every user-triggered command — a `:Wolf*` Ex-command, a
//! `require('wolf').*()` Lua call, or the `close_window` that
//! `runtime/nvim/init.lua`'s `:q`/`:qa` handler fires — and the GTK front-end
//! drains that queue on each render wake. This module owns the DECISION of what
//! each command means to the GTK shell; the widget work of carrying it out lives
//! in the GTK glue layer. Keeping the decision here (data in, data out, no GTK
//! types) is exactly what lets it be unit-tested headlessly, the same way
//! `layout.rs`, `geometry.rs` and `cursor.rs` keep their decisions pure.
//!
//! ## Totality is the contract
//!
//! [`ui_action_for`] is a `match` with **no `_` arm**. Appending a variant to
//! [`wolf_core::Command`] is therefore an `E0004` compile error in `wolf-gnome`
//! until the new command is classified — as a dispatched action, as
//! [`UiAction::Deferred`] with a reason, or as [`UiAction::NoGnomeEquivalent`]
//! with a reason. That is the same totality guard `pane_view::apply_intents` uses
//! for `LayoutIntent`, and it exists because a silently-dropped command is
//! indistinguishable from a broken one. The unit tests below add a runtime
//! belt-and-braces check (`every_registered_command_maps`) so the guard survives
//! even if someone were to add a catch-all arm.
//!
//! ## The three classes
//!
//! - **Dispatched** — the twelve commands `wolf-gnome` performs today.
//! - [`UiAction::Deferred`] — implemented on macOS and planned on GTK; the string
//!   names the concrete blocker so the deferral is auditable rather than a shrug.
//! - [`UiAction::NoGnomeEquivalent`] — has no GNOME counterpart and will never be
//!   implemented; the string says why.
//! - **Front-end-only rows** — an [`crate::actions::ACTION_TABLE`] row that carries
//!   no [`wolf_core::Command`] at all, because the behaviour is the front end's own
//!   rather than something Neovim can queue: the two navigation rows, whose visit
//!   log is the front end's own view of the core, and About, which the registry has
//!   no command for. Those resolve through
//!   [`ui_action_for_front_end_action`], which is the ONLY other way an action is
//!   installed.
//!
//! Every `Deferred` / `NoGnomeEquivalent` reason is DISTINCT, so no two commands
//! can collapse into one indistinguishable diagnostic.

use wolf_core::Command;

/// What the GTK shell does in response to one drained [`wolf_core::Command`].
///
/// The payload-free variants are the actions `wolf-gnome` performs today; the two
/// payload variants carry a human-readable reason that is printed (never
/// swallowed) when such a command is drained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    /// Open the in-window file picker overlay.
    OpenFilePicker,
    /// Open a native `gtk::FileDialog` to open a single file.
    OpenFileDialog,
    /// Open a native `gtk::FileDialog` to select a folder (re-roots the project).
    OpenFolderDialog,
    /// Create a new untitled Markdown buffer in the focused pane.
    NewUntitledBuffer,
    /// Close the focused editor pane (Neovim performs the `:close`).
    ClosePane,
    /// Close the application window (via `close-request`, never `destroy`).
    CloseWindow,
    /// Minimize the application window.
    MinimizeWindow,
    /// Quit the application by closing every window cleanly.
    QuitApplication,
    /// Open the in-window command-palette overlay.
    OpenCommandPalette,
    /// Open the in-window buffer-picker overlay.
    OpenBufferPicker,
    /// Open the in-window project-search overlay.
    OpenSearchOverlay,
    /// Move the focused pane one entry BACK in its navigation history.
    HistoryBack,
    /// Move the focused pane one entry FORWARD in its navigation history.
    HistoryForward,
    /// Show or hide the file-tree sidebar.
    ToggleSidebar,
    /// Present the About dialog carrying the bundled fonts' licence notices.
    ShowAbout,
    /// Implemented on macOS, planned on GTK; the string names the blocker.
    Deferred(&'static str),
    /// Has no GNOME equivalent and will never be implemented.
    NoGnomeEquivalent(&'static str),
}

/// Maps a drained [`wolf_core::Command`] to the [`UiAction`] the GTK shell
/// performs for it.
///
/// TOTAL by construction: the implementation is a `match` with **no `_` arm**, so
/// a newly appended `Command` variant fails to compile here until it is
/// classified. See the module docs.
#[must_use]
pub fn ui_action_for(command: Command) -> UiAction {
    match command {
        Command::NewWindow => UiAction::Deferred(
            "wolf-gnome is a single-window application; multi-window is the feat-list 'Multiple \
             windows' row",
        ),
        Command::CloseWindow => UiAction::CloseWindow,
        Command::QuitApplication => UiAction::QuitApplication,
        Command::HideApplication => UiAction::NoGnomeEquivalent(
            "GNOME has no application-level hide; the shell owns window visibility",
        ),
        Command::MinimizeWindow => UiAction::MinimizeWindow,
        Command::GoToFile => UiAction::OpenFilePicker,
        Command::OpenFile => UiAction::OpenFileDialog,
        Command::OpenFolder => UiAction::OpenFolderDialog,
        Command::ClosePane => UiAction::ClosePane,
        Command::NewBuffer => UiAction::NewUntitledBuffer,
        Command::ResetFontSize => UiAction::Deferred(
            "resetting the font size requires re-measuring CellMetrics and re-gridding every pane",
        ),
        Command::IncreaseFontSize => UiAction::Deferred(
            "increasing the font size requires re-measuring CellMetrics and re-gridding every pane",
        ),
        Command::DecreaseFontSize => UiAction::Deferred(
            "decreasing the font size requires re-measuring CellMetrics and re-gridding every pane",
        ),
        Command::CommandPalette => UiAction::OpenCommandPalette,
        Command::Copy => UiAction::Deferred(
            "the GTK clipboard bridge (gdk::Clipboard + copy_visual_selection) is not built",
        ),
        Command::Paste => UiAction::Deferred(
            "the GTK clipboard bridge (gdk::Clipboard read + WolfCore::paste) is not built",
        ),
        Command::BufferPicker => UiAction::OpenBufferPicker,
        Command::NewTab => UiAction::Deferred(
            "GTK native tabs are not built; wolf-gnome binds exactly one nvim tabpage",
        ),
        Command::Preview => {
            UiAction::Deferred("the GTK preview pane (a WebKitGTK second pane kind) is not built")
        }
        Command::Edit => UiAction::Deferred(
            "editing the source is the preview pane's inverse and needs that pane kind first",
        ),
        Command::Terminal => UiAction::Deferred(
            "pane_view drops LayoutIntent::DrawerOpen, so toggling would open a :terminal window \
             GTK never paints",
        ),
        Command::PreviewInBrowser => UiAction::Deferred(
            "the loopback preview server needs a GTK URI launcher and the preview row first",
        ),
        Command::FullWidth => UiAction::Deferred(
            "the ~90-column content cap is a front-end feature wolf-gnome does not implement",
        ),
        Command::ToggleSidebar => UiAction::ToggleSidebar,
        Command::SidebarShowProject => UiAction::Deferred(
            "wolf-gnome's sidebar has no panel switcher (there is no search panel to switch away \
             from)",
        ),
        Command::SearchInProject => UiAction::OpenSearchOverlay,
    }
}

/// The UI action a FRONT-END-ONLY action row performs — an
/// [`crate::actions::ACTION_TABLE`] row whose `command` is `None`. `None` for
/// anything else.
///
/// None of these rows has a [`wolf_core::Command`] variant, so
/// [`crate::shell::AppShell::install_actions`] cannot reach them through
/// [`ui_action_for`]. The navigation rows have none because a pane's visit log is
/// the front end's own view of the core; About has none because the registry
/// carries no About command and the dialog is this front-end's own surface. A row
/// that resolves in NEITHER place is skipped with a printed reason while its
/// accelerator is still registered, which swallows the chord and does nothing at
/// all — strictly worse than not binding it.
///
/// This lives here rather than in [`crate::actions`] because this module owns
/// [`UiAction`]; `actions.rs` stays free of the classification type. Only the test
/// module below imports `actions`, so there is no module cycle.
#[must_use]
pub fn ui_action_for_front_end_action(action: &str) -> Option<UiAction> {
    match action {
        "win.history-back" => Some(UiAction::HistoryBack),
        "win.history-forward" => Some(UiAction::HistoryForward),
        "win.about" => Some(UiAction::ShowAbout),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;

    use wolf_core::COMMAND_SPECS;

    /// The commands `wolf-gnome` DISPATCHES, by canonical `CommandSpec::id`,
    /// sorted. Ids (not variants) so the failure message names the registry row a
    /// reader can grep for.
    const EXPECTED_DISPATCHED_IDS: [&str; 12] = [
        "buffer_picker",
        "close_pane",
        "close_window",
        "command_palette",
        "go_to_file",
        "minimize_window",
        "new_buffer",
        "open_file",
        "open_folder",
        "quit_application",
        "search_in_project",
        "toggle_sidebar",
    ];

    /// What a [`Case`] row expects [`ui_action_for`] to return.
    ///
    /// The two unimplemented classes are matched on their CLASS, not on their
    /// reason string: the reason is documentation, and pinning ~18 sentences here
    /// would make a wording tweak look like a behaviour regression. The reason
    /// strings' actual contract (non-empty, clause-shaped, unique) is asserted by
    /// [`unimplemented_reasons_are_distinct_and_non_empty`].
    enum Want {
        /// An exact, payload-free [`UiAction`] — a command this unit DISPATCHES.
        Exact(UiAction),
        /// `UiAction::Deferred(_)` — planned on GTK, blocked on something concrete.
        Deferred,
        /// `UiAction::NoGnomeEquivalent(_)` — will never be implemented on GNOME.
        NoGnomeEquivalent,
    }

    /// One `ui_action_for` table row: a human-readable description, the command,
    /// and the expected classification.
    struct Case {
        desc: &'static str,
        command: Command,
        want: Want,
    }

    /// The COMPLETE `Command -> UiAction` classification, one row per
    /// [`wolf_core::Command`] variant in discriminant order (plan section 3.2).
    ///
    /// This table is the authority for the classification; `ui_action_table`
    /// checks the implementation against it and `every_registered_command_maps`
    /// checks the table against the registry, so neither can drift alone.
    const CASES: &[Case] = &[
        Case {
            desc: "0 NewWindow: wolf-gnome is single-window",
            command: Command::NewWindow,
            want: Want::Deferred,
        },
        Case {
            desc: "1 CloseWindow: the :qa path — closes the window",
            command: Command::CloseWindow,
            want: Want::Exact(UiAction::CloseWindow),
        },
        Case {
            desc: "2 QuitApplication: closes every window cleanly",
            command: Command::QuitApplication,
            want: Want::Exact(UiAction::QuitApplication),
        },
        Case {
            desc: "3 HideApplication: GNOME has no app-level hide",
            command: Command::HideApplication,
            want: Want::NoGnomeEquivalent,
        },
        Case {
            desc: "4 MinimizeWindow: minimizes the window",
            command: Command::MinimizeWindow,
            want: Want::Exact(UiAction::MinimizeWindow),
        },
        Case {
            desc: "5 GoToFile: opens the in-window file picker",
            command: Command::GoToFile,
            want: Want::Exact(UiAction::OpenFilePicker),
        },
        Case {
            desc: "6 OpenFile: native file dialog",
            command: Command::OpenFile,
            want: Want::Exact(UiAction::OpenFileDialog),
        },
        Case {
            desc: "7 OpenFolder: native folder dialog (re-roots the project)",
            command: Command::OpenFolder,
            want: Want::Exact(UiAction::OpenFolderDialog),
        },
        Case {
            desc: "8 ClosePane: the :q path — closes the focused pane",
            command: Command::ClosePane,
            want: Want::Exact(UiAction::ClosePane),
        },
        Case {
            desc: "9 NewBuffer: new untitled Markdown buffer",
            command: Command::NewBuffer,
            want: Want::Exact(UiAction::NewUntitledBuffer),
        },
        Case {
            desc: "10 ResetFontSize: needs CellMetrics re-measurement",
            command: Command::ResetFontSize,
            want: Want::Deferred,
        },
        Case {
            desc: "11 IncreaseFontSize: needs CellMetrics re-measurement",
            command: Command::IncreaseFontSize,
            want: Want::Deferred,
        },
        Case {
            desc: "12 DecreaseFontSize: needs CellMetrics re-measurement",
            command: Command::DecreaseFontSize,
            want: Want::Deferred,
        },
        Case {
            desc: "13 CommandPalette: opens the in-window command-palette overlay",
            command: Command::CommandPalette,
            want: Want::Exact(UiAction::OpenCommandPalette),
        },
        Case {
            desc: "14 Copy: no GTK clipboard bridge yet",
            command: Command::Copy,
            want: Want::Deferred,
        },
        Case {
            desc: "15 Paste: no GTK clipboard bridge yet",
            command: Command::Paste,
            want: Want::Deferred,
        },
        Case {
            desc: "16 BufferPicker: opens the in-window buffer-picker overlay",
            command: Command::BufferPicker,
            want: Want::Exact(UiAction::OpenBufferPicker),
        },
        Case {
            desc: "17 NewTab: no GTK native tabs",
            command: Command::NewTab,
            want: Want::Deferred,
        },
        Case {
            desc: "18 Preview: no GTK preview pane kind",
            command: Command::Preview,
            want: Want::Deferred,
        },
        Case {
            desc: "19 Edit: the preview pane's inverse, same blocker",
            command: Command::Edit,
            want: Want::Deferred,
        },
        Case {
            desc: "20 Terminal: pane_view drops LayoutIntent::DrawerOpen",
            command: Command::Terminal,
            want: Want::Deferred,
        },
        Case {
            desc: "21 PreviewInBrowser: needs the preview row + a URI launcher",
            command: Command::PreviewInBrowser,
            want: Want::Deferred,
        },
        Case {
            desc: "22 FullWidth: wolf-gnome has no content-width cap",
            command: Command::FullWidth,
            want: Want::Deferred,
        },
        Case {
            desc: "23 ToggleSidebar: shows or hides the file-tree sidebar",
            command: Command::ToggleSidebar,
            want: Want::Exact(UiAction::ToggleSidebar),
        },
        Case {
            desc: "24 SidebarShowProject: the sidebar has no panel switcher",
            command: Command::SidebarShowProject,
            want: Want::Deferred,
        },
        Case {
            desc: "25 SearchInProject: opens the in-window project-search overlay",
            command: Command::SearchInProject,
            want: Want::Exact(UiAction::OpenSearchOverlay),
        },
    ];

    /// Whether `got` satisfies the row's [`Want`].
    fn satisfies(want: &Want, got: UiAction) -> bool {
        match want {
            Want::Exact(expected) => got == *expected,
            Want::Deferred => matches!(got, UiAction::Deferred(_)),
            Want::NoGnomeEquivalent => matches!(got, UiAction::NoGnomeEquivalent(_)),
        }
    }

    /// Whether `reason` opens like a SENTENCE (`Xy…`) rather than as a
    /// mid-sentence clause.
    ///
    /// An ALL-CAPS opener is deliberately allowed so `HideApplication`'s
    /// "GNOME has no application-level hide…" passes: the reason is interpolated
    /// into `"wolf-gnome: command {cmd:?} not dispatched: {reason}"`, where a
    /// proper noun / acronym reads correctly but a capitalized ordinary word
    /// ("Resetting the font size…") does not.
    fn is_sentence_cased(reason: &str) -> bool {
        let mut chars = reason.chars();
        let (Some(first), Some(second)) = (chars.next(), chars.next()) else {
            return false;
        };
        first.is_uppercase() && second.is_lowercase()
    }

    /// Pins the classification of EVERY command, one row per variant.
    ///
    /// The length assertion against `COMMAND_SPECS` is what makes the table
    /// non-vacuous: a newly appended core command cannot be absorbed silently.
    #[test]
    fn ui_action_table() {
        assert_eq!(
            CASES.len(),
            COMMAND_SPECS.len(),
            "the ui_action_for table must carry exactly one row per registered command \
             (COMMAND_SPECS has {} rows, the table has {}). A new wolf_core::Command needs a \
             classification here AND a match arm in `ui_action_for`.",
            COMMAND_SPECS.len(),
            CASES.len()
        );

        for case in CASES {
            let got = ui_action_for(case.command);
            assert!(
                satisfies(&case.want, got),
                "case: {} — ui_action_for({:?}) returned {got:?}, which does not match the \
                 classification pinned in plan section 3.2",
                case.desc,
                case.command
            );
        }
    }

    /// Belt-and-braces for the totality guard: every registered command appears in
    /// the table above EXACTLY once.
    ///
    /// The `_`-free match in `ui_action_for` already makes a new `Command` variant
    /// a compile error, but that guard evaporates the moment anyone adds a
    /// catch-all arm. This check does not, because it walks the registry rather
    /// than the match.
    #[test]
    fn every_registered_command_maps() {
        for spec in COMMAND_SPECS {
            let hits = CASES
                .iter()
                .filter(|case| case.command == spec.command)
                .count();
            assert_eq!(
                hits, 1,
                "registered command {:?} (id {:?}) appears {hits} times in the ui_action_for \
                 table; it must appear exactly once. A missing row means a command was added to \
                 wolf-core without being classified for the GTK front-end; a duplicate row means \
                 two rows disagree about what it does.",
                spec.command, spec.id
            );
        }
    }

    /// The set of commands `wolf-gnome` actually dispatches is exactly
    /// [`EXPECTED_DISPATCHED_IDS`] — no more (nothing quietly promoted) and no
    /// fewer (nothing quietly demoted to `Deferred`).
    #[test]
    fn dispatched_commands_are_the_expected_set() {
        let mut dispatched: Vec<&'static str> = Vec::new();
        for spec in COMMAND_SPECS {
            match ui_action_for(spec.command) {
                UiAction::Deferred(_) | UiAction::NoGnomeEquivalent(_) => {}
                _ => dispatched.push(spec.id),
            }
        }
        dispatched.sort_unstable();

        let mut expected: Vec<&'static str> = EXPECTED_DISPATCHED_IDS.to_vec();
        expected.sort_unstable();

        assert_eq!(
            dispatched, expected,
            "the set of DISPATCHED commands changed. wolf-gnome dispatches exactly twelve commands; \
             everything else must be Deferred(reason) or NoGnomeEquivalent(reason) so the drain \
             is never silent. If a command was genuinely implemented, tick its feat-list row and \
             update this list with it."
        );
    }

    /// Every `Deferred` / `NoGnomeEquivalent` reason is non-empty, reads as a
    /// mid-sentence clause, and is UNIQUE.
    ///
    /// Uniqueness is load-bearing twice over: it stops two commands collapsing
    /// into one indistinguishable diagnostic, and it keeps a future pedantic
    /// `clippy::match_same_arms` run clean.
    #[test]
    fn unimplemented_reasons_are_distinct_and_non_empty() {
        let mut seen: HashSet<&'static str> = HashSet::new();

        for spec in COMMAND_SPECS {
            let (class, reason) = match ui_action_for(spec.command) {
                UiAction::Deferred(reason) => ("Deferred", reason),
                UiAction::NoGnomeEquivalent(reason) => ("NoGnomeEquivalent", reason),
                _ => continue,
            };

            assert!(
                !reason.trim().is_empty(),
                "{class} reason for {:?} (id {:?}) is empty. The reason is printed when the \
                 command is drained; an empty one makes the drop indistinguishable from a bug.",
                spec.command,
                spec.id
            );
            assert!(
                !is_sentence_cased(reason),
                "{class} reason for {:?} (id {:?}) is sentence-cased ({reason:?}). It is \
                 interpolated mid-sentence into \"wolf-gnome: command {{cmd:?}} not dispatched: \
                 {{reason}}\", so it must start lowercase (an ALL-CAPS proper noun such as \
                 \"GNOME\" is fine).",
                spec.command,
                spec.id
            );
            assert!(
                seen.insert(reason),
                "{class} reason for {:?} (id {:?}) duplicates another command's reason \
                 ({reason:?}). Reasons must be DISTINCT so the printed diagnostic names the real \
                 blocker.",
                spec.command,
                spec.id
            );
        }
    }

    /// Action names that are NOT declared in [`crate::actions::ACTION_TABLE`] at
    /// all, used to pin [`ui_action_for_front_end_action`]'s `_ => None` arm.
    ///
    /// The first is a plausible near-miss for a real row and the second is
    /// well-formed but invented; a function that answered `Some` for either would
    /// install an action for a name no table declares, which `gio` accepts silently
    /// and the menu then renders as a dead entry.
    const UNDECLARED_ACTIONS: [&str; 3] = [
        "win.history-backward",
        "win.history-sideways",
        "app.not-a-real-action",
    ];

    /// Every FRONT-END-ONLY row of [`crate::actions::ACTION_TABLE`] — a row whose
    /// `command` is `None` — resolves through [`ui_action_for_front_end_action`],
    /// and nothing else does.
    ///
    /// ## Why both directions matter
    ///
    /// `shell::install_actions` resolves each row through exactly two places: a row
    /// with a command goes to [`ui_action_for`], and a command-less row comes here.
    /// A row that resolves in NEITHER is skipped with a printed reason, so its
    /// accelerator is registered against an action that was never installed — the
    /// chord is then swallowed in the capture phase and does nothing at all, which
    /// is strictly worse than not binding it. The forward direction is what catches
    /// that.
    ///
    /// The reverse direction catches the opposite mistake: an arm here for a name
    /// that also carries a `Command` would give one action two classifications, and
    /// which one wins would depend on the order `install_actions` happens to try
    /// them in.
    ///
    /// The non-vacuity guard is load-bearing. With no command-less rows in the
    /// table both loops run zero times, and a `ui_action_for_front_end_action` that
    /// was nothing but `_ => None` would satisfy the whole test.
    #[test]
    fn front_end_action_rows_are_classified() {
        use crate::actions::ACTION_TABLE;

        let front_end_rows: Vec<&'static str> = ACTION_TABLE
            .iter()
            .filter(|binding| binding.command.is_none())
            .map(|binding| binding.action)
            .collect();

        assert!(
            !front_end_rows.is_empty(),
            "ACTION_TABLE declares no command-less rows at all, so both directions below run zero \
             times and this test proves nothing — a `ui_action_for_front_end_action` consisting of \
             nothing but `_ => None` would pass it. The front-end-only rows are the two navigation \
             ones (`win.history-back` / `win.history-forward`), whose history lives in the front \
             end's own view of the core, and `win.about`, which the registry carries no command \
             for. If every one of them has gained a `wolf_core::Command`, this function has no \
             purpose left and should go rather than be left guarding nothing."
        );

        // --- Forward: every command-less row is installable ------------------
        //
        // A `Vec` rather than a `HashSet` because `UiAction` is deliberately not
        // `Hash` — it carries `&'static str` payloads whose contract is equality and
        // distinctness, not hashing — and the list is short enough that the linear
        // scan costs nothing. Its length is not asserted here: the row set is
        // `ACTION_TABLE`'s to define, and the loop is over whatever it declares.
        let mut seen: Vec<UiAction> = Vec::new();
        for action in &front_end_rows {
            let resolved = ui_action_for_front_end_action(action);
            let Some(ui_action) = resolved else {
                panic!(
                    "ACTION_TABLE row {action:?} carries no `command`, so `shell::install_actions` \
                     resolves it through `ui_action_for_front_end_action` — which answered None. \
                     The action is therefore never installed, while its accelerator IS registered: \
                     the capture-phase chord matcher resolves the chord, swallows the key, and \
                     activates nothing. Add an arm for {action:?} here, or give the row a Command."
                )
            };
            assert!(
                !seen.contains(&ui_action),
                "two command-less ACTION_TABLE rows resolve to the SAME UiAction ({ui_action:?}); \
                 {action:?} is the second. Back and Forward are the rows this bites: collapsing \
                 them means one of the two chords navigates the wrong way, which looks like a \
                 broken history rather than a broken classification."
            );
            seen.push(ui_action);
        }

        // --- Reverse: nothing ELSE resolves through this function -------------
        for binding in ACTION_TABLE {
            let resolved = ui_action_for_front_end_action(binding.action);
            if binding.command.is_some() {
                assert_eq!(
                    resolved, None,
                    "ACTION_TABLE row {:?} carries the command {:?} AND resolves through \
                     `ui_action_for_front_end_action` to {resolved:?}. One action must have exactly \
                     one classification: `shell::install_actions` takes the command branch first, \
                     so this arm is dead code that silently disagrees with `ui_action_for` — and \
                     the day the branch order changes, the action's behaviour changes with it.",
                    binding.action,
                    binding.command
                );
            }
        }

        for action in UNDECLARED_ACTIONS {
            assert_eq!(
                ui_action_for_front_end_action(action),
                None,
                "`ui_action_for_front_end_action({action:?})` answered a UiAction for a name that \
                 ACTION_TABLE does not declare. Every installed action must come from a declared \
                 row: an undeclared one has no accelerator and no menu entry, so it is reachable \
                 from nowhere, and a near-miss name here means a real row's arm was misspelled and \
                 the real row now resolves to None."
            );
        }
    }
}
