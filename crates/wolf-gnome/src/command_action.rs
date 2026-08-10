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
//! - **Dispatched** — the twenty-one commands `wolf-gnome` performs today.
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
    /// Open a new TAB in this window, over the window's EXISTING core, rooted at
    /// the window's own folder.
    ///
    /// The exact opposite of [`UiAction::NewWindow`] in what it inherits, and the
    /// pair is easy to confuse: a tab shares the window's captive Neovim, its
    /// project and its sidebar, while a window shares nothing at all. The
    /// directory is [`crate::tabs_ui::new_tab_cwd`]'s answer, and the press is
    /// gated by [`crate::tabs_ui::new_tab_press`] because `create_tabpage` is a
    /// blocking RPC.
    NewTab,
    /// Open a FRESH, EMPTY application window with its own core, at `$HOME`.
    ///
    /// It inherits nothing from the window it was asked from — not its working
    /// directory, not its layout, not its size — because that is what macOS's
    /// `makeNewWindow` does and parity is this row's whole subject. Inheriting a
    /// project is [`UiAction::NewTab`]'s behaviour.
    NewWindow,
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
    /// Yank the visual-mode selection over a BLOCKING RPC and write it to the
    /// system clipboard.
    CopySelection,
    /// Read the system clipboard ASYNCHRONOUSLY and insert it through
    /// `nvim_paste`.
    PasteClipboard,
    /// Toggle the focused buffer's content width between the column cap and the
    /// whole pane.
    ToggleFullWidth,
    /// Show or hide the terminal drawer, the full-width shell Neovim opens along
    /// the bottom of the pane area.
    ToggleTerminal,
    /// Open the Markdown preview for the focused buffer, at the placement
    /// [`crate::preview_ui::preview_placement`] gives.
    OpenPreview,
    /// Leave the focused preview pane for its source file, at the placement
    /// [`crate::preview_ui::edit_placement`] gives.
    EditSource,
    /// Serve the focused Markdown buffer from core's loopback preview server over a
    /// BLOCKING RPC and hand the resulting `http://127.0.0.1:<port>/…` address to the
    /// platform's URI launcher, at the gate
    /// [`crate::preview_ui::browser_press`] gives.
    OpenInBrowser,
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
        Command::NewWindow => UiAction::NewWindow,
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
        Command::Copy => UiAction::CopySelection,
        Command::Paste => UiAction::PasteClipboard,
        Command::BufferPicker => UiAction::OpenBufferPicker,
        Command::NewTab => UiAction::NewTab,
        Command::Preview => UiAction::OpenPreview,
        Command::Edit => UiAction::EditSource,
        Command::Terminal => UiAction::ToggleTerminal,
        Command::PreviewInBrowser => UiAction::OpenInBrowser,
        Command::FullWidth => UiAction::ToggleFullWidth,
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
    const EXPECTED_DISPATCHED_IDS: [&str; 21] = [
        "buffer_picker",
        "close_pane",
        "close_window",
        "command_palette",
        "copy",
        "edit",
        "full_width",
        "go_to_file",
        "minimize_window",
        "new_buffer",
        "new_tab",
        "new_window",
        "open_file",
        "open_folder",
        "paste",
        "preview",
        "preview_in_browser",
        "quit_application",
        "search_in_project",
        "terminal",
        "toggle_sidebar",
    ];

    /// What a [`Case`] row expects [`ui_action_for`] to return.
    ///
    /// The two unimplemented classes are matched on their CLASS, not on their
    /// reason string: the reason is documentation, and pinning one sentence of prose
    /// per unimplemented command here would make a wording tweak look like a
    /// behaviour regression. A count is deliberately not given — it moves with every
    /// classification change, which is exactly what this file does. The reason
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
            desc: "0 NewWindow: opens a fresh, empty window with its own core",
            command: Command::NewWindow,
            want: Want::Exact(UiAction::NewWindow),
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
            desc: "14 Copy: yanks the visual selection and writes it to the system clipboard",
            command: Command::Copy,
            want: Want::Exact(UiAction::CopySelection),
        },
        Case {
            desc: "15 Paste: reads the system clipboard and inserts it through nvim_paste",
            command: Command::Paste,
            want: Want::Exact(UiAction::PasteClipboard),
        },
        Case {
            desc: "16 BufferPicker: opens the in-window buffer-picker overlay",
            command: Command::BufferPicker,
            want: Want::Exact(UiAction::OpenBufferPicker),
        },
        Case {
            desc: "17 NewTab: appends a native tab to THIS window, over the one core it already \
                   has, rooted at the window's own folder",
            command: Command::NewTab,
            want: Want::Exact(UiAction::NewTab),
        },
        Case {
            desc: "18 Preview: opens the Markdown preview pane",
            command: Command::Preview,
            want: Want::Exact(UiAction::OpenPreview),
        },
        Case {
            desc: "19 Edit: the preview pane's inverse — leaves it for the source file",
            command: Command::Edit,
            want: Want::Exact(UiAction::EditSource),
        },
        Case {
            desc: "20 Terminal: shows or hides the terminal drawer",
            command: Command::Terminal,
            want: Want::Exact(UiAction::ToggleTerminal),
        },
        Case {
            desc: "21 PreviewInBrowser: serves the buffer over loopback and launches the URL",
            command: Command::PreviewInBrowser,
            want: Want::Exact(UiAction::OpenInBrowser),
        },
        Case {
            desc: "22 FullWidth: toggles the focused buffer's content width",
            command: Command::FullWidth,
            want: Want::Exact(UiAction::ToggleFullWidth),
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
    ///
    /// FALSIFYING EDIT for the row that carries the content-width cap: classify
    /// `Command::FullWidth` as `UiAction::Deferred`. MEASURED: `case: 22 FullWidth:
    /// toggles the focused buffer's content width — ui_action_for(FullWidth)
    /// returned Deferred("the ~90-column content cap is a front-end feature
    /// wolf-gnome does not implement"), which does not match the classification
    /// pinned in plan section 3.2`. This test was run ALONE under that edit, so the
    /// failure above is its own and not
    /// [`dispatched_commands_are_the_expected_set`]'s showing through it.
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
    ///
    /// FALSIFYING EDIT: classify `Command::PreviewInBrowser` as `UiAction::Deferred`,
    /// and run this test ALONE so its evidence is its own. Expected: the sorted `left`
    /// comes back one element short of `right`, the missing element being
    /// `"preview_in_browser"` in sorted position between `"preview"` and
    /// `"quit_application"`. That command is the one this unit promoted, so it is the
    /// edit a reverting change would actually make.
    ///
    /// MEASURED with the same shape on a different row, which is what says the diff
    /// really is one element and really names the command rather than reporting that
    /// "the set changed": classifying `Command::FullWidth` as `UiAction::Deferred` puts
    /// `"full_width"` in sorted position between `"edit"` and `"go_to_file"` and
    /// changes nothing else.
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
            "the set of DISPATCHED commands changed. wolf-gnome dispatches exactly twenty-one \
             commands; everything else must be Deferred(reason) or NoGnomeEquivalent(reason) so \
             the drain is never silent. If a command was genuinely implemented, tick its feat-list \
             row and update this list with it. A one-element diff naming \"new_window\" \
             means New Window was demoted back to Deferred, in which case the coordinator's \
             `open_new_window` is unreachable from the primary menu's New Window item, from the \
             palette row and from `:WolfNewWindow` alike — the application is single-window again \
             however many windows the coordinator can hold. That demotion is NOT isolated in this \
             module: `ui_action_table`'s case 0 reddens beside this test, since one \
             reclassification falsifies both, and the pair moving together is what says the \
             classification changed rather than the registry. A one-element diff naming \
             \"new_tab\" \
             means the native-tabs command was demoted back to Deferred, in which case \
             Ctrl+Shift+T, the primary menu's New Tab item, the palette's New Tab row and \
             `:WolfNewTab` all stop opening a tab — the window is single-tab again however many \
             pages its TabView can hold, and `shell::AppShell::new_tab` is reachable from nowhere. \
             That demotion is NOT isolated in this module: `ui_action_table`'s case 17 reddens \
             beside this test, since one reclassification falsifies both, and the pair moving \
             together is what says the classification changed rather than the registry. It also \
             reddens `palette::tests::palette_rows_query_new_ranks_the_new_buffer_row_first`, \
             whose actionable-row count for the query \"new\" drops from three rows to two while \
             its TOP row is unchanged — that third red is what tells this demotion apart from a \
             registry rename, which would move the top row instead. \
             `actions::bound_set_is_exactly_this_unit` stays GREEN through it, because the \
             accelerator row is a separate switch from this classification. A one-element diff \
             naming \
             \"terminal\" \
             means the terminal drawer was demoted back to Deferred, in which case the drawer is \
             unreachable from the Ctrl+backtick chord, from the primary menu's Terminal item and \
             from the command palette alike — and `actions::bound_set_is_exactly_this_unit` stays \
             GREEN while it happens, because the accelerator row is a separate switch from this \
             classification. That demotion is NOT isolated in this module: `ui_action_table`'s \
             case 20 reddens beside this test, since one reclassification falsifies both, and the \
             pair moving together is what says the classification changed rather than the \
             registry. A one-element diff naming \"preview_in_browser\" \
             means the loopback-preview command was demoted back to Deferred, in which case \
             `preview_ui::browser_press` and `shell::AppShell::preview_in_browser` are unreachable \
             from any command AND the palette's `prv` collision disappears — so \
             `palette::tests::palette_rows_for_the_preview_smoke_rank_the_named_command_first` \
             reddens beside this one. MEASURED: it reddens on its `actionable_rows` clause, NOT \
             on its unfiltered floor. `rank_palette` runs BEFORE the classification filter, so \
             `Preview in Browser` is still among its candidates and the floor of 2 still holds; \
             it is `palette_rows`, which applies the filter, that drops to one row. That pairing \
             is how this failure is told apart from a registry change. A two-element diff naming \
             \"preview\" and \"edit\" \
             means the Markdown preview pair was demoted back to Deferred, in which case \
             `preview_ui`'s press seams are unreachable from any command. A one-element diff \
             naming \"copy\" or \"paste\" \
             means the clipboard bridge was demoted back to Deferred, in which case the chord \
             falls through to Neovim as <C-c> or <C-v> and the app prints its own \
             \"command Copy not dispatched:\" line on stderr — that stderr line is what tells this \
             failure apart from `actions::bound_set_is_exactly_this_unit`'s, which reddens instead \
             when the ACCELERATOR was demoted to Reserved."
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

    /// The two [`UiAction`]s the COORDINATOR performs, which is the whole of what
    /// an `app.`-namespace row may classify to.
    ///
    /// `Coordinator::perform` is deliberately NOT exhaustive over [`UiAction`] —
    /// every other variant belongs to a window's own shell — so a third arm added
    /// there without being added here makes this list the stale half.
    const APP_SCOPED_UI_ACTIONS: [UiAction; 2] = [UiAction::NewWindow, UiAction::QuitApplication];

    /// Every `app.`-namespace [`crate::actions::ACTION_TABLE`] row classifies to a
    /// [`UiAction`] the coordinator has an arm for.
    ///
    /// The two installation paths are mirror images that read the SAME table:
    /// [`crate::coordinator::Coordinator::install_app_actions`] installs the `app.`
    /// rows once per process and `shell::AppShell::install_actions` installs the
    /// `win.` rows once per window. Both skip a row whose command classifies to
    /// `Deferred` / `NoGnomeEquivalent`, and both install everything else — so an
    /// `app.` row classifying to any OTHER real `UiAction` is installed, listed in
    /// the primary menu, and does nothing at all when activated: the coordinator's
    /// `perform` falls through to its `other =>` arm, prints
    /// "app-scoped ui action not performed: … needs a window and belongs in the
    /// win. namespace", and returns. That is the palette's "a surface that lists a
    /// no-op" failure, arriving through the menu instead.
    ///
    /// `crate::actions::tests::app_scoped_rows_are_performable_without_a_window`
    /// is the other half of the same rule and the two are read together: that one
    /// pins WHICH rows are `app.`-scoped, this one pins what those rows may mean.
    ///
    /// FALSIFYING EDIT: point the `app.new-window` row at `Command::NewBuffer`.
    /// PREDICTED: this test reddens naming `app.new-window` and
    /// `NewUntitledBuffer`. PREDICTED NOT ISOLATED —
    /// `the_new_window_row_is_menu_only_and_listed` reddens on its `command`
    /// assertion, `menu_labels_match_the_registry` on the "New Window" label
    /// against the `New` title, and `command_action_round_trip` on two rows
    /// claiming `Command::NewBuffer`. The pairing is the disclosure: a red here
    /// ALONE would mean the row kept its command and the CLASSIFICATION of that
    /// command moved, which is a different bug in a different file.
    #[test]
    fn app_scoped_commands_classify_to_an_action_the_coordinator_performs() {
        use crate::actions::ACTION_TABLE;

        let mut checked = 0_usize;
        for binding in ACTION_TABLE {
            let Some(("app", _)) = binding.action.split_once('.') else {
                continue;
            };
            let Some(command) = binding.command else {
                panic!(
                    "the app-scoped row {:?} carries no `wolf_core::Command`. \
                     `Coordinator::install_app_actions` resolves a row through its command and \
                     nothing else — there is no `ui_action_for_front_end_action` fallback in the \
                     `app.` half — so the row is printed and skipped, and its menu item renders \
                     insensitive while its accelerator (if any) is still registered and swallows \
                     the chord.",
                    binding.action
                )
            };

            let ui_action = ui_action_for(command);
            assert!(
                APP_SCOPED_UI_ACTIONS.contains(&ui_action),
                "the app-scoped row {:?} carries {command:?}, which classifies to {ui_action:?} — \
                 not one of {APP_SCOPED_UI_ACTIONS:?}, the arms `Coordinator::perform` matches. \
                 The row is still INSTALLED (both installers skip only Deferred and \
                 NoGnomeEquivalent), so the primary menu offers a working-looking item that \
                 prints \"app-scoped ui action not performed\" and does nothing. Either give the \
                 behaviour a `win.` row, where `AppShell::perform` has a window to act on, or add \
                 the arm to `Coordinator::perform` AND to the list above.",
                binding.action
            );
            checked += 1;
        }

        assert!(
            checked > 0,
            "ACTION_TABLE declares no `app.`-namespace row at all, so the loop above ran zero \
             times and this test proves nothing. Two rows are expected — `app.new-window` and \
             `app.quit` — and if both really have become window-scoped then \
             `Coordinator::install_app_actions` installs nothing, Ctrl+Shift+Q is unbound, and \
             this test should go with it rather than be left guarding an empty set."
        );
    }
}
