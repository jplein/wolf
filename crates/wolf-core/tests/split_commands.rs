//! Integration tests of the Split Horizontal and Split Vertical commands in `wolf-core`:
//! `WolfCore::split_pane` from an editor window, a terminal pane, a Markdown preview pane and
//! the terminal drawer, each read back as the `Split` layout intent and the kind of the new
//! window, beside a real, hermetic Neovim and a real `/bin/sh` in a real pty.
//!
//! Split Horizontal is Vim's `:split` (panes stacked) and Split Vertical is `:vsplit` (side by
//! side), so every case pins the orientation as well as the new window's kind.
//!
//! ## Premises about the components
//!
//! Each is measured by these tests against the real component, not assumed:
//!
//! - Neovim's `:split` and `:vsplit` give the new window the buffer of the window they split,
//!   so an editor split shows the same buffer number and path as its origin.
//! - A window on a preview's scratch buffer is reported in `Snapshot::previews` like the
//!   window `open_preview` bound it into, so a split preview pane is a preview of the same
//!   source.
//! - The router's split runs the origin session's program again in the directory the origin
//!   shell is in, so a split of an interactive `/bin/sh` that `cd`'d is an interactive
//!   `/bin/sh` in that directory, as `terminal_input.rs` measures for the chords.
//! - The terminal drawer is Neovim's current window once `toggle_terminal` opens it, and its
//!   placeholder carries `b:wolf_terminal_id` like a pane's.

mod common;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::{
    active_window, interactive_sh, open_session, poll_frame, poll_until, run_line, settle_for,
    start_core_at, start_core_with_temp_md, wait_for_text, win_terminal, FlushWaiter,
    FLUSH_TIMEOUT, POLL_INTERVAL,
};
use wolf_core::{LayoutIntent, SplitOrientation, TerminalId, TerminalTarget, WolfCore};

/// How long a split that must do nothing is given to do something anyway.
const NEGATIVE_SETTLE: Duration = Duration::from_millis(1500);

/// How long the freshly opened drawer is left before its window count is read.
const DRAWER_SETTLE: Duration = Duration::from_secs(1);

/// How long after a split's barrier before the new window's kind is read.
const SPLIT_SETTLE: Duration = Duration::from_millis(500);

/// What the preview cases' Markdown source holds.
const PREVIEW_SOURCE: &str = "# Split preview\n\nA paragraph under the heading.\n";

/// The subdirectory the terminal case's origin shell `cd`s into before the split.
const ORIGIN_SUBDIR: &str = "split-command-origin";

/// Starts a core running `/bin/sh` as its default shell, in a fresh canonical cwd.
fn start_core() -> (WolfCore, FlushWaiter, PathBuf) {
    common::pin_default_shell();
    let cwd = std::fs::canonicalize(common::fresh_cwd()).expect("failed to canonicalize the cwd");
    let (core, waiter) = start_core_at(cwd.clone());
    (core, waiter, cwd)
}

/// Drains layout intents until `pick` accepts one, returning what it picked. Panics with
/// `desc` and every intent seen after [`FLUSH_TIMEOUT`].
fn wait_for_intent<T>(core: &WolfCore, desc: &str, pick: impl Fn(&LayoutIntent) -> Option<T>) -> T {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut seen = Vec::new();
    loop {
        for intent in core.drain_intents() {
            if let Some(picked) = pick(&intent) {
                return picked;
            }
            seen.push(intent);
        }
        if Instant::now() >= deadline {
            panic!("timed out after {FLUSH_TIMEOUT:?} waiting for {desc}; intents seen: {seen:?}");
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Runs `split_pane(orientation)` with `origin` active, checks the `Split` intent it yields, and
/// returns the new window.
fn split_and_check_intent(core: &WolfCore, orientation: SplitOrientation, origin: i64) -> i64 {
    let _ = core.drain_intents();
    core.split_pane(orientation);
    let (got, from, new_win) = wait_for_intent(
        core,
        &format!(
            "a Split intent from split_pane({orientation:?}); none means the command split \
             nothing, or split without reporting it"
        ),
        |intent| match intent {
            LayoutIntent::Split {
                orientation,
                from,
                win,
            } => Some((*orientation, *from, *win)),
            _ => None,
        },
    );
    assert_eq!(
        (got, from),
        (orientation, origin),
        "split_pane({orientation:?}) must split the active window {origin} as {orientation:?}, \
         not {got:?} from {from}: the other orientation means Split Horizontal and Split Vertical \
         are swapped, another window means the command split something other than the active \
         pane."
    );
    assert_ne!(
        new_win, origin,
        "the Split intent must name a new window, not the origin {origin} itself."
    );
    new_win
}

/// Waits until Neovim has finished handling the split and its notifications have landed.
///
/// The `Split` intent is sent from inside the split, so an emit from the new window's `WinNew`
/// can show it before the rest of the command ran. A blocking request is answered only after
/// the command, and the settle lets the notifications sent before the answer land.
fn settle_after_split(core: &WolfCore, waiter: &FlushWaiter) {
    assert_ne!(
        core.current_tabpage(),
        0,
        "the barrier request after the split failed, so the new window's state could be read \
         before Neovim finished the command."
    );
    settle_for(waiter, SPLIT_SETTLE);
}

/// `path` canonicalized, failing on a path that does not resolve.
fn canonical(path: &std::path::Path) -> PathBuf {
    std::fs::canonicalize(path)
        .unwrap_or_else(|e| panic!("failed to canonicalize {}: {e}", path.display()))
}

/// Splits the editor window on a temp `.md` and checks the new window shows the same buffer.
fn editor_split_shows_its_buffer(orientation: SplitOrientation) {
    let (core, waiter, file) = start_core_with_temp_md("editor split source line\n");
    let path = canonical(file.path());
    let origin = active_window(&core, &waiter);
    let origin_bufnr = poll_until(
        &waiter,
        "the origin window's win_buffers entry to show the opened file",
        || {
            core.read_snapshot()
                .win_buffers
                .get(&origin)
                .map(|buffer| (buffer.bufnr, buffer.path.clone()))
        },
        |entry| matches!(entry, Some((bufnr, Some(p))) if *bufnr > 0 && *p == path),
    )
    .expect("poll_until returned only once the origin's entry was set")
    .0;

    let new_win = split_and_check_intent(&core, orientation, origin);
    settle_after_split(&core, &waiter);
    let shown = poll_until(
        &waiter,
        "the new window's win_buffers entry",
        || {
            core.read_snapshot()
                .win_buffers
                .get(&new_win)
                .map(|buffer| (buffer.bufnr, buffer.path.clone(), buffer.terminal))
        },
        Option::is_some,
    );
    assert_eq!(
        shown,
        Some((origin_bufnr, Some(path.clone()), None)),
        "the {orientation:?} split's new window {new_win} must show the origin's buffer \
         {origin_bufnr} ({}) and no session: another buffer means the new pane is not a window \
         on the same buffer, as <C-w>s and <C-w>v give, and a session means an editor pane was \
         split as a terminal.",
        path.display()
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// The text of the first output row starting with `prefix`, polling until there is one.
fn output_row(core: &WolfCore, waiter: &FlushWaiter, id: TerminalId, prefix: &str) -> String {
    let frame = poll_frame(
        core,
        waiter,
        id,
        &format!("an output row starting with {prefix:?}"),
        |frame| (0..frame.rows).any(|row| frame.row_text(row).starts_with(prefix)),
    );
    (0..frame.rows)
        .map(|row| frame.row_text(row))
        .find(|text| text.starts_with(prefix))
        .expect("poll_frame returned only once such a row existed")
}

/// Splits a terminal pane whose shell sits in [`ORIGIN_SUBDIR`] and checks the new window shows
/// a new, live session. With `check_cwd`, also checks the new shell starts in that directory.
fn terminal_split_starts_a_new_session(orientation: SplitOrientation, check_cwd: bool) {
    let (core, waiter, cwd) = start_core();
    let origin = active_window(&core, &waiter);
    let id = open_session(&core, TerminalTarget::Window(0), interactive_sh());
    if check_cwd {
        std::fs::create_dir(cwd.join(ORIGIN_SUBDIR)).expect("failed to create the origin subdir");
        run_line(
            &core,
            id,
            &format!("cd {ORIGIN_SUBDIR} && echo moved-$((3+4))"),
        );
        wait_for_text(&core, &waiter, id, "moved-7");
    }
    poll_until(
        &waiter,
        "the origin window to carry the opened session",
        || win_terminal(&core, origin),
        |bound| *bound == Some(id),
    );

    let new_win = split_and_check_intent(&core, orientation, origin);
    let (live, bound) = poll_until(
        &waiter,
        "a second live session bound in the split's new window. The new window carrying the \
         origin's own session, with one session live, means the terminal pane was split as an \
         editor window on the same placeholder rather than through the router",
        || {
            let mut live = core.live_terminals();
            live.sort();
            (live, win_terminal(&core, new_win))
        },
        |(live, bound)| live.len() == 2 && bound.is_some_and(|new| new != id),
    );
    let new = bound.expect("poll_until returned only once the new window carried a session");
    assert!(
        live.contains(&id) && live.contains(&new),
        "both the origin {id:?} and the new session {new:?} must be live after the split, not \
         {live:?}: a missing origin means the split hung up the pane it split."
    );

    if check_cwd {
        run_line(&core, new, "echo \"at:$(basename \"$(pwd -P)\")\"");
        let at = output_row(&core, &waiter, new, "at:");
        assert_eq!(
            at,
            format!("at:{ORIGIN_SUBDIR}"),
            "the new session must start where the origin shell is, {ORIGIN_SUBDIR}, not {at:?}: \
             the core cwd's name means the split ignored the origin shell's directory."
        );
    }

    core.shutdown().expect("shutdown should be Ok");
}

/// Splits a preview pane and checks the new window previews the same source.
fn preview_split_previews_the_same_source(orientation: SplitOrientation) {
    let (core, waiter, file) = start_core_with_temp_md(PREVIEW_SOURCE);
    let source = canonical(file.path());
    let origin = active_window(&core, &waiter);
    assert!(
        core.open_preview() > 0,
        "open_preview() on the Markdown source must return the scratch buffer's number; 0 means \
         the preview could not be opened, so the case has no preview pane to split."
    );
    poll_until(
        &waiter,
        "the active window to become the preview pane (a premise of the case)",
        || {
            core.read_snapshot()
                .previews
                .get(&origin)
                .map(|info| canonical(&info.source_path))
        },
        |shown| shown.as_ref() == Some(&source),
    );

    let new_win = split_and_check_intent(&core, orientation, origin);
    settle_after_split(&core, &waiter);
    let previewed = poll_until(
        &waiter,
        "the split's new window to be reported as a preview. Never means it shows something \
         other than the preview's scratch buffer",
        || {
            core.read_snapshot()
                .previews
                .get(&new_win)
                .map(|info| canonical(&info.source_path))
        },
        Option::is_some,
    );
    assert_eq!(
        previewed.as_ref(),
        Some(&source),
        "the {orientation:?} split of a preview of {} must preview the same source, not \
         {previewed:?}.",
        source.display()
    );
    assert_eq!(
        win_terminal(&core, new_win),
        None,
        "a preview pane's split must not be a terminal pane."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Split Horizontal in an editor window gives a stacked window on the same buffer.
#[test]
fn split_horizontal_in_an_editor_window_shows_its_buffer() {
    editor_split_shows_its_buffer(SplitOrientation::Horizontal);
}

/// Split Vertical in an editor window gives a side-by-side window on the same buffer.
#[test]
fn split_vertical_in_an_editor_window_shows_its_buffer() {
    editor_split_shows_its_buffer(SplitOrientation::Vertical);
}

/// Split Horizontal in a terminal pane starts a new session, where the origin shell is.
#[test]
fn split_horizontal_in_a_terminal_pane_starts_a_new_session() {
    terminal_split_starts_a_new_session(SplitOrientation::Horizontal, true);
}

/// Split Vertical in a terminal pane starts a new session.
#[test]
fn split_vertical_in_a_terminal_pane_starts_a_new_session() {
    terminal_split_starts_a_new_session(SplitOrientation::Vertical, false);
}

/// Split Horizontal in a preview pane gives a preview of the same source.
#[test]
fn split_horizontal_in_a_preview_pane_previews_the_same_source() {
    preview_split_previews_the_same_source(SplitOrientation::Horizontal);
}

/// Split Vertical in a preview pane gives a preview of the same source.
#[test]
fn split_vertical_in_a_preview_pane_previews_the_same_source() {
    preview_split_previews_the_same_source(SplitOrientation::Vertical);
}

/// Both split commands do nothing from the terminal drawer: no new window, session or `Split`.
#[test]
fn split_commands_in_the_drawer_do_nothing() {
    let (core, waiter, _cwd) = start_core();
    let editor = active_window(&core, &waiter);
    core.toggle_terminal();
    let (drawer, id) = poll_until(
        &waiter,
        "the drawer to open on a session as Neovim's current window",
        || {
            let snap = core.read_snapshot();
            let drawer = snap
                .win_buffers
                .iter()
                .find(|(_, buffer)| buffer.is_drawer)
                .map(|(win, buffer)| (*win, buffer.terminal));
            (drawer, snap.active_win)
        },
        |(drawer, active)| {
            matches!(drawer, Some((win, Some(_))) if Some(*win) == *active && *win != editor)
        },
    )
    .0
    .expect("poll_until returned only once a drawer window existed");
    let id = id.expect("poll_until returned only once the drawer carried a session");
    settle_for(&waiter, DRAWER_SETTLE);
    let windows_before = core.read_snapshot().win_buffers.len();
    let _ = core.drain_intents();

    core.split_pane(SplitOrientation::Horizontal);
    core.split_pane(SplitOrientation::Vertical);
    // Both splits are queued ahead of this focus, so it landing means Neovim handled them.
    core.focus_win(editor);
    poll_until(
        &waiter,
        "the editor window to become active again after the drawer's splits",
        || core.read_snapshot().active_win,
        |active| *active == Some(editor),
    );
    settle_for(&waiter, NEGATIVE_SETTLE);

    let splits: Vec<LayoutIntent> = core
        .drain_intents()
        .into_iter()
        .filter(|intent| matches!(intent, LayoutIntent::Split { .. }))
        .collect();
    assert_eq!(
        (
            core.read_snapshot().win_buffers.len(),
            core.live_terminals(),
            splits.len()
        ),
        (windows_before, vec![id], 0),
        "split_pane in the drawer {drawer} must change nothing: (window count, live sessions, \
         Split intents) must stay ({windows_before}, [{id:?}], 0). A second session means the \
         router split the drawer's session; more windows or a Split intent mean Neovim split the \
         drawer window. Split intents: {splits:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}
