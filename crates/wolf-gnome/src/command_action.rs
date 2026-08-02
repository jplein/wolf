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
//! - **Dispatched** — the eight commands `wolf-gnome` performs today.
//! - [`UiAction::Deferred`] — implemented on macOS and planned on GTK; the string
//!   names the concrete blocker so the deferral is auditable rather than a shrug.
//! - [`UiAction::NoGnomeEquivalent`] — has no GNOME counterpart and will never be
//!   implemented; the string says why.
//!
//! Every `Deferred` / `NoGnomeEquivalent` reason is DISTINCT, so no two rows of
//! the README's command table can silently collapse into one.

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
        Command::CommandPalette => UiAction::Deferred(
            "the GTK command-palette overlay is not built (wolf_core::rank_palette is already \
             linked)",
        ),
        Command::Copy => UiAction::Deferred(
            "the GTK clipboard bridge (gdk::Clipboard + copy_visual_selection) is not built",
        ),
        Command::Paste => UiAction::Deferred(
            "the GTK clipboard bridge (gdk::Clipboard read + WolfCore::paste) is not built",
        ),
        Command::BufferPicker => UiAction::Deferred(
            "the GTK buffer-picker overlay is not built (wolf_core::rank_buffers is already \
             linked)",
        ),
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
        Command::ToggleSidebar => UiAction::Deferred(
            "a user-toggleable sidebar is the feat-list 'Unified toolbar' / sidebar row",
        ),
        Command::SidebarShowProject => UiAction::Deferred(
            "wolf-gnome's sidebar has no panel switcher (there is no search panel to switch away \
             from)",
        ),
        Command::SearchInProject => UiAction::Deferred(
            "the GTK project-search panel is not built (wolf_core::search is already linked)",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;

    use wolf_core::COMMAND_SPECS;

    /// The commands `wolf-gnome` DISPATCHES in this unit, by canonical
    /// `CommandSpec::id`, sorted. Ids (not variants) so the failure message names
    /// the registry row a reader can grep for. See plan section 3.2's
    /// "Dispatched now (8)" line.
    const EXPECTED_DISPATCHED_IDS: [&str; 8] = [
        "close_pane",
        "close_window",
        "go_to_file",
        "minimize_window",
        "new_buffer",
        "open_file",
        "open_folder",
        "quit_application",
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
            desc: "13 CommandPalette: no GTK palette overlay yet",
            command: Command::CommandPalette,
            want: Want::Deferred,
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
            desc: "16 BufferPicker: no GTK buffer-picker overlay yet",
            command: Command::BufferPicker,
            want: Want::Deferred,
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
            desc: "23 ToggleSidebar: no user-toggleable sidebar yet",
            command: Command::ToggleSidebar,
            want: Want::Deferred,
        },
        Case {
            desc: "24 SidebarShowProject: the sidebar has no panel switcher",
            command: Command::SidebarShowProject,
            want: Want::Deferred,
        },
        Case {
            desc: "25 SearchInProject: no GTK project-search panel",
            command: Command::SearchInProject,
            want: Want::Deferred,
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

    /// The set of commands `wolf-gnome` actually dispatches is exactly the eight
    /// of plan section 3.2 — no more (nothing quietly promoted) and no fewer
    /// (nothing quietly demoted to `Deferred`).
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
            "the set of DISPATCHED commands changed. This unit dispatches exactly eight commands \
             (plan section 3.2); everything else must be Deferred(reason) or \
             NoGnomeEquivalent(reason) so the drain is never silent. If a command was genuinely \
             implemented, tick its feat-list row and update this list and the README table \
             together."
        );
    }

    /// Every `Deferred` / `NoGnomeEquivalent` reason is non-empty, reads as a
    /// mid-sentence clause, and is UNIQUE.
    ///
    /// Uniqueness is load-bearing twice over: it stops two rows of the README's
    /// command table from collapsing into one indistinguishable entry, and it
    /// keeps a future pedantic `clippy::match_same_arms` run clean.
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
                 ({reason:?}). Reasons must be DISTINCT so the README's command table cannot \
                 collapse two rows, and so the printed diagnostic names the real blocker.",
                spec.command,
                spec.id
            );
        }
    }
}
