//! Terminal panes: a pane whose window shows a `wolf-core` terminal session, painted from the
//! session's [`wolf_core::TerminalFrame`] rather than from the placeholder's grid.
//!
//! The submodules are the pure seams ([`style`], [`damage`], [`geometry`], [`input`]) and the
//! display-free painter ([`canvas`]); the widget glue lives in [`crate::pane_view`],
//! [`crate::window`] and [`crate::shell`]. `specs/terminal-panes.md` is authoritative.

pub mod canvas;
pub mod damage;
pub mod geometry;
pub mod input;
pub mod style;

use std::path::PathBuf;

use wolf_core::TerminalCommand;

/// The test-only variable naming a program the first pane of each window runs as a terminal
/// session; unset or blank, no session opens.
pub const SMOKE_TERMINAL_ENV: &str = "WOLF_SMOKE_TERMINAL";

/// The command [`SMOKE_TERMINAL_ENV`]'s value `raw` asks for: the trimmed text as the program,
/// with no arguments, or `None` when `raw` is absent or blank.
#[must_use]
pub fn smoke_terminal_command(raw: Option<&str>) -> Option<TerminalCommand> {
    let program = raw?.trim();
    if program.is_empty() {
        return None;
    }
    Some(TerminalCommand {
        program: PathBuf::from(program),
        args: Vec::new(),
    })
}
