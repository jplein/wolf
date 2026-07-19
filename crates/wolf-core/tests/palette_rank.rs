//! Pure, headless tests for the command-palette candidate assembly + ranking
//! (`wolf_core::rank_palette` and the `PaletteRow` / `PaletteRowKind` /
//! `PaletteWindow` types). No mocks, no Neovim, no filesystem — `rank_palette` is
//! IO-free, exactly like `rank::rank_matches` which it reuses.
//!
//! These live in a dedicated integration file (with its own `palette_rank` BUILD
//! target) rather than an in-module `#[cfg(test)] mod tests` inside the NEW
//! `src/palette.rs`, to avoid colliding with the coder who creates that file. This
//! mirrors the crate's existing precedent: the pure `command.rs` registry tests
//! live in `tests/commands.rs` for the same "don't collide with the coder" reason.
//!
//! ## Dependency on coder work (plan §Q3 / §5)
//!
//! None of this exists yet — the coder adds `pub mod palette;` (+ the re-exports
//! this file imports) in `crates/wolf-core/src/lib.rs`, and adds the
//! `//crates/wolf-core:palette_rank` `rust_test` target to `BUILD.bazel`. This
//! file does NOT touch non-test source. It assumes the plan's exact signatures:
//!
//! - `pub struct PaletteRow { pub display: String, pub mru_key: String,
//!   pub kind: PaletteRowKind, pub command: Option<Command>,
//!   pub window_index: Option<u16>, pub path: Option<PathBuf> }`
//!   (derives `Debug` for the panic messages here)
//! - `pub enum PaletteRowKind { Static, SwitchWindow, OpenRecent }`
//!   (derives `Debug, PartialEq, Eq`)
//! - `pub struct PaletteWindow { pub index: u16, pub cwd: String }`
//! - `pub fn rank_palette(recent_dirs: &[PathBuf], windows: &[PaletteWindow],
//!   command_mru_recent: &[String], query: &str, max: usize) -> Vec<PaletteRow>`

use std::path::PathBuf;

use wolf_core::{rank_palette, Command, PaletteRow, PaletteRowKind, PaletteWindow, COMMAND_SPECS};

/// The number of STATIC rows the palette assembles: every `COMMAND_SPECS` row
/// EXCEPT `Command::CommandPalette` (which is excluded from its own list). With
/// 21 registered commands (Copy/Paste/BufferPicker/NewTab/Preview/Edit/Terminal
/// appended at 14/15/16/17/18/19/20) that is 20 static rows: exactly
/// `COMMAND_SPECS.len() - 1`.
///
/// The `- 1` is NOT arithmetic coincidence — it holds because `rank_palette`
/// (`src/palette.rs`) iterates `for spec in COMMAND_SPECS` with exactly ONE
/// `continue`, guarded by `spec.command == Command::CommandPalette`. There is no
/// other filter: no `MenuGroup` gating, no "is a Markdown buffer active" check, no
/// dedup. So every spec but the palette's own invoking command contributes one
/// static row.
///
/// Preview, Edit and Terminal are all NEW palette static rows (OD4: unlike the
/// macOS menu items, which gate Preview/Edit Source on the active buffer/pane, the
/// palette does not gate any spec — it lists them all).
const STATIC_COMMAND_COUNT: usize = 20;

/// A `max` large enough to never truncate the assembled candidate set in tests
/// that assert on membership rather than truncation.
const A_LARGE_MAX: usize = 100;

/// Finds the row (if any) whose static `command` equals `cmd`.
fn row_for_command(rows: &[PaletteRow], cmd: Command) -> Option<&PaletteRow> {
    rows.iter().find(|r| r.command == Some(cmd))
}

/// Position of the row whose static `command` equals `cmd`, or `None`.
fn pos_of_command(rows: &[PaletteRow], cmd: Command) -> Option<usize> {
    rows.iter().position(|r| r.command == Some(cmd))
}

/// The first row of the given `kind`, or `None`.
fn first_of_kind(rows: &[PaletteRow], kind: PaletteRowKind) -> Option<&PaletteRow> {
    rows.iter().find(|r| r.kind == kind)
}

/// (a) `Command::CommandPalette` is EXCLUDED from the assembled candidate list,
/// while every OTHER static command IS present exactly once.
#[test]
fn command_palette_is_excluded_and_static_rows_present() {
    let rows = rank_palette(&[], &[], &[], "", A_LARGE_MAX);

    assert!(
        row_for_command(&rows, Command::CommandPalette).is_none(),
        "the palette must EXCLUDE its own invoking command (CommandPalette): {rows:?}"
    );

    // Every other registered command has exactly one static row.
    for spec in COMMAND_SPECS {
        if spec.command == Command::CommandPalette {
            continue;
        }
        let count = rows
            .iter()
            .filter(|r| r.command == Some(spec.command))
            .count();
        assert_eq!(
            count, 1,
            "command {:?} must appear as exactly one static row",
            spec.command
        );
    }

    let static_rows = rows
        .iter()
        .filter(|r| r.kind == PaletteRowKind::Static)
        .count();
    assert_eq!(
        static_rows, STATIC_COMMAND_COUNT,
        "exactly {STATIC_COMMAND_COUNT} static rows (all commands but CommandPalette)"
    );

    // BufferPicker (the appended discriminant 16) is a REACHABLE palette static
    // row: it is NOT the palette's own invoking command, so it must be present.
    let buffer_picker = row_for_command(&rows, Command::BufferPicker)
        .expect("BufferPicker must appear as a palette static row");
    assert_eq!(
        buffer_picker.display, "Buffer Picker…",
        "the BufferPicker static row's display is its registry title"
    );

    // NewTab (the appended discriminant 17) is likewise a REACHABLE palette static
    // row: it is NOT the palette's own invoking command, so it must be present
    // (OD4). Its display is the registry title "New Tab".
    let new_tab = row_for_command(&rows, Command::NewTab)
        .expect("NewTab must appear as a palette static row");
    assert_eq!(
        new_tab.display, "New Tab",
        "the NewTab static row's display is its registry title"
    );
}

/// (b) A STATIC row carries the right kind, command, empty dynamic payloads, and
/// its label/mru_key come from the registry `title`/`id`.
#[test]
fn static_row_fields_are_correct() {
    let rows = rank_palette(&[], &[], &[], "", A_LARGE_MAX);

    let row = row_for_command(&rows, Command::NewWindow).expect("NewWindow must have a static row");
    assert_eq!(row.kind, PaletteRowKind::Static, "kind");
    assert_eq!(row.display, "New Window", "display == registry title");
    assert_eq!(row.mru_key, "new_window", "mru_key == registry id");
    assert_eq!(row.window_index, None, "static row has no window index");
    assert_eq!(row.path, None, "static row has no path");
}

/// (b) A SWITCH_WINDOW row carries the window index, no command/path, and its
/// label/mru_key are built from the (non-empty) cwd.
#[test]
fn switch_window_row_fields_are_correct() {
    let windows = [PaletteWindow {
        index: 3,
        cwd: "/home/x".to_string(),
    }];

    let rows = rank_palette(&[], &windows, &[], "", A_LARGE_MAX);

    let row = first_of_kind(&rows, PaletteRowKind::SwitchWindow)
        .expect("a switch-window row must be present");
    assert_eq!(row.display, "Switch to Window: /home/x", "display");
    assert_eq!(row.mru_key, "switch_window:/home/x", "mru_key");
    assert_eq!(row.window_index, Some(3), "window index carried through");
    assert_eq!(row.command, None, "switch-window row has no static command");
    assert_eq!(row.path, None, "switch-window row has no path");
}

/// (b) An OPEN_RECENT row carries the path, no command/window index, and its
/// label/mru_key are built from the recent-dir path.
#[test]
fn open_recent_row_fields_are_correct() {
    let recent = [PathBuf::from("/home/proj")];

    let rows = rank_palette(&recent, &[], &[], "", A_LARGE_MAX);

    let row = first_of_kind(&rows, PaletteRowKind::OpenRecent)
        .expect("an open-recent row must be present");
    assert_eq!(row.display, "Open Recent Folder: /home/proj", "display");
    assert_eq!(row.mru_key, "open_recent:/home/proj", "mru_key");
    assert_eq!(row.path, Some(PathBuf::from("/home/proj")), "path carried");
    assert_eq!(
        row.window_index, None,
        "open-recent row has no window index"
    );
    assert_eq!(row.command, None, "open-recent row has no static command");
}

/// (c) A window with an EMPTY cwd is labeled "Untitled" and its mru_key collapses
/// to the bare `switch_window:` prefix (the accepted minor MRU quirk).
#[test]
fn empty_cwd_window_is_untitled() {
    let windows = [PaletteWindow {
        index: 1,
        cwd: String::new(),
    }];

    let rows = rank_palette(&[], &windows, &[], "", A_LARGE_MAX);

    let row = first_of_kind(&rows, PaletteRowKind::SwitchWindow)
        .expect("a switch-window row must be present for the empty-cwd window");
    assert_eq!(
        row.display, "Switch to Window: Untitled",
        "empty cwd => Untitled label"
    );
    assert_eq!(
        row.mru_key, "switch_window:",
        "empty cwd => bare switch_window: key"
    );
    assert_eq!(row.window_index, Some(1), "window index still carried");
}

/// (d) A `command_mru_recent` entry floats its row to the FRONT even against a
/// stronger textual match on another row — MRU is the absolute leading key
/// (inherited from `rank_matches`).
///
/// Query "in":
///   - "Increase Font Size" (increase_font_size): component/word start "In..."
///     -> a STRONG match (high score), but NOT in the MRU list.
///   - "Minimize" (minimize_window): "in" only as a contiguous substring
///     (m-IN-imize) -> a WEAK match (low score), but it IS the one MRU entry.
///
/// The weak-but-MRU row must still rank ahead of the strong non-MRU row.
#[test]
fn mru_recent_floats_a_row_first() {
    let mru = ["minimize_window".to_string()];

    let rows = rank_palette(&[], &[], &mru, "in", A_LARGE_MAX);

    let p_mru =
        pos_of_command(&rows, Command::MinimizeWindow).expect("Minimize must match query 'in'");
    let p_strong = pos_of_command(&rows, Command::IncreaseFontSize)
        .expect("Increase Font Size must match query 'in'");
    assert!(
        p_mru < p_strong,
        "the MRU row (minimize_window) must rank ahead of the stronger non-MRU \
         match (increase_font_size): minimize@{p_mru}, increase@{p_strong}"
    );
}

/// (e) Empty recent dirs + no windows => ONLY static rows (the palette handles
/// empties gracefully; `rank_palette` still returns the full static set).
#[test]
fn empty_inputs_yield_only_static_rows() {
    let rows = rank_palette(&[], &[], &[], "", A_LARGE_MAX);

    assert_eq!(
        rows.len(),
        STATIC_COMMAND_COUNT,
        "with no windows and no recent dirs, only the static rows remain"
    );
    assert!(
        rows.iter().all(|r| r.kind == PaletteRowKind::Static),
        "every row must be a static-command row: {rows:?}"
    );
}

/// (f) A non-empty query filters to the AND-subsequence matches (reusing
/// `rank_matches` semantics). Query "font size" -> terms ["font","size"]: only
/// the three font-size commands contain BOTH terms.
#[test]
fn query_filters_to_and_subsequence_matches() {
    let rows = rank_palette(&[], &[], &[], "font size", A_LARGE_MAX);

    for cmd in [
        Command::ResetFontSize,
        Command::IncreaseFontSize,
        Command::DecreaseFontSize,
    ] {
        assert!(
            row_for_command(&rows, cmd).is_some(),
            "{cmd:?} matches both 'font' and 'size' and must be kept"
        );
    }

    assert!(
        row_for_command(&rows, Command::NewWindow).is_none(),
        "New Window matches neither 'font' nor 'size' and must be dropped: {rows:?}"
    );
    assert_eq!(
        rows.len(),
        3,
        "exactly the three font-size commands survive the AND filter: {rows:?}"
    );
}

/// (g) `max` truncates the ranked output to at most `max` rows.
#[test]
fn max_truncates_output() {
    let truncated = 3;
    let rows = rank_palette(&[], &[], &[], "", truncated);
    assert_eq!(
        rows.len(),
        truncated,
        "max must truncate the assembled candidate set to {truncated} rows"
    );
}
