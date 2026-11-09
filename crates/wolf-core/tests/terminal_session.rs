//! Integration tests of `wolf-core`'s terminal sessions and the frame they publish: a real
//! `/bin/sh` in a real pty, driven through the `WolfCore` session API, beside a real, hermetic
//! Neovim that holds the placeholder windows.
//!
//! Covered: output reaching the frame's cells and keys reaching the shell, a resize reaching
//! both the frame and the shell, the shell's environment and cwd, id allocation, a failed
//! spawn, the placeholder's binding in `Snapshot.win_buffers`, `pane_status` for a terminal
//! pane, row versions, publishing through the wake without `seq`, the publish coalescing, the
//! render hold of synchronized output, the colours taken from the editor, and typed
//! `:terminal`, `:term` and `:ter` opening a session in the current window.
//!
//! ## Premises about the components
//!
//! Each is measured by these tests against the real component, not assumed:
//!
//! - `/bin/sh` is bash 3.2 in POSIX mode on macOS and the dev shell's `sh` on Linux. With no
//!   argument on a tty it is interactive and echoes what it is typed, so a test looks for the
//!   output of an expression (`$((6*7))`) that the typed text does not contain.
//! - `stty -echo` stops the tty echoing input, and with `ICRNL` still on a CR ends a `read`.
//! - `sleep` accepts a fractional argument (`sleep 0.2`) on both platforms.
//! - A plain `:hi` changes no `ColorScheme` state, so `:doautocmd ColorScheme` is what makes
//!   `wolf_theme` emit again, as `theme.rs` does. Changing `Normal` makes Neovim send
//!   `default_colors_set` in its next redraw batch.
//! - Paths are compared after `canonicalize` or with `pwd -P`, since macOS temp dirs sit
//!   behind the `/var` → `/private/var` link.
//! - Neovim's own `:terminal` makes a buffer whose `&buftype` is `terminal`, while a session's
//!   placeholder is `nofile`, so `&buftype` tells which of the two a typed word opened.

mod common;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::{
    active_window, frame_of, frame_text, grid_of_win, interactive_sh, open_session, poll_frame,
    poll_until, probe_values, run_line, send_ex, settle_for, sh_script, start_core_at,
    start_core_recording, term_key, terminal_windows, wait_for_text, FlushWaiter, POLL_INTERVAL,
};
use wolf_core::{
    Key, Rgb, SplitOrientation, TerminalCommand, TerminalId, TerminalTarget, WolfCore, NO_NAME,
    TERMINAL_PUBLISH_INTERVAL, TERMINAL_RENDER_HOLD_MAX,
};

/// How long a session is left alone before a measurement that a stray publish would spoil.
const QUIET_SETTLE: Duration = Duration::from_millis(500);

/// A vertical split of Neovim's current window.
const SPLIT_CURRENT: TerminalTarget = TerminalTarget::Split {
    win: 0,
    orientation: SplitOrientation::Vertical,
};

/// Starts a core in a fresh cwd, returning it with its waiter and the canonical cwd.
fn start_core() -> (WolfCore, FlushWaiter, PathBuf) {
    let cwd = std::fs::canonicalize(common::fresh_cwd()).expect("failed to canonicalize the cwd");
    let (core, waiter) = start_core_at(cwd.clone());
    (core, waiter, cwd)
}

/// Raises `updatetime` so no `CursorHold` autocmd redraws while a test measures wakes and `seq`.
fn quiet_neovim(core: &WolfCore, waiter: &FlushWaiter) {
    send_ex(core, "set updatetime=600000");
    settle_for(waiter, QUIET_SETTLE);
}

/// What an interactive shell prints for an expression reaches the frame's cells, and the
/// Enter that runs it reaches the shell through `terminal_key`.
#[test]
fn output_reaches_cells_and_keys_reach_the_shell() {
    let (core, waiter, _cwd) = start_core();
    let id = open_session(&core, TerminalTarget::Window(0), interactive_sh());

    run_line(&core, id, "echo wolf-$((6*7))");
    let frame = wait_for_text(&core, &waiter, id, "wolf-42");
    assert!(
        (0..frame.rows).any(|row| {
            let text = frame.row_text(row);
            text.contains("wolf-42") && !text.contains("echo")
        }),
        "`wolf-42` must be on an output row of its own, not only on the echoed command line; the \
         frame reads:\n{}",
        frame_text(&frame)
    );
    assert_eq!(
        frame.shell, "sh",
        "the frame's shell must be the program's basename, \"sh\", not {:?}.",
        frame.shell
    );
    assert_eq!(
        frame.exited, None,
        "the interactive shell is still running, so `exited` must be None."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `resize_terminal` resizes the frame and the pty: the frame is 100x30 and `stty size` in
/// the shell prints `30 100`.
#[test]
fn resize_reaches_the_frame_and_the_shell() {
    let (core, waiter, _cwd) = start_core();
    let id = open_session(&core, TerminalTarget::Window(0), interactive_sh());
    let before = frame_of(&core, id);
    assert_eq!(
        (before.cols, before.rows),
        (80, 24),
        "a session that was never resized must start at the default 80x24, not {}x{}.",
        before.cols,
        before.rows
    );

    core.resize_terminal(id, 100, 30, 9, 18);
    poll_frame(
        &core,
        &waiter,
        id,
        "the frame to be 100x30 after resize_terminal(100, 30); 80x24 means the emulator was \
         never resized",
        |frame| (frame.cols, frame.rows) == (100, 30),
    );

    run_line(&core, id, "stty size");
    let frame = poll_frame(
        &core,
        &waiter,
        id,
        "`stty size` to print the pty's size",
        |frame| frame_text(frame).contains("30 100") || frame_text(frame).contains("24 80"),
    );
    assert!(
        frame_text(&frame).contains("30 100"),
        "`stty size` must print `30 100` once the session is 100x30; `24 80` means the pty was \
         never given the size (no TIOCSWINSZ). The frame reads:\n{}",
        frame_text(&frame)
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// The shell runs with `TERM=xterm-256color`, `COLORTERM=truecolor` and `TERM_PROGRAM=Wolf`
/// over the app's own values, in the tabpage's cwd.
#[test]
fn the_shell_gets_the_terminal_environment_and_the_tabpage_cwd() {
    // Decoys, so a session that only inherits the test's environment shows them instead.
    std::env::set_var("TERM", "dumb");
    std::env::set_var("COLORTERM", "decoy-colorterm");
    std::env::set_var("TERM_PROGRAM", "decoy-term-program");
    let (core, waiter, cwd) = start_core();
    let script = format!(
        "printf '%s|%s|%s|' \"$TERM\" \"$COLORTERM\" \"$TERM_PROGRAM\"; \
         if [ \"$(pwd -P)\" = '{}' ]; then echo cwd-match; else echo cwd-mismatch; pwd -P; fi; \
         exec sleep 1000",
        cwd.display()
    );
    let id = open_session(&core, TerminalTarget::Window(0), sh_script(&script));

    let frame = wait_for_text(&core, &waiter, id, "|cwd-m");
    let text = frame_text(&frame);
    assert!(
        text.contains("xterm-256color|truecolor|Wolf|"),
        "the shell must see TERM=xterm-256color, COLORTERM=truecolor and TERM_PROGRAM=Wolf; a \
         decoy (dumb, decoy-colorterm, decoy-term-program) means that variable was inherited \
         rather than set. The frame reads:\n{text}"
    );
    assert!(
        text.contains("|cwd-match"),
        "the shell must start in the tabpage's cwd, {}; cwd-mismatch means `current_dir` was not \
         set (the printed path follows it). The frame reads:\n{text}",
        cwd.display()
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Ids count up from 1 per core and are never reused, and `live_terminals` lists the running
/// sessions in ascending order.
#[test]
fn ids_count_up_and_are_never_reused() {
    let (core, waiter, _cwd) = start_core();
    let sleeper = || sh_script("exec sleep 1000");

    let first = open_session(&core, TerminalTarget::Window(0), sleeper());
    let second = open_session(&core, SPLIT_CURRENT, sleeper());
    assert_eq!(
        (first, second),
        (TerminalId(1), TerminalId(2)),
        "a core's first two sessions must be ids 1 and 2."
    );

    core.close_terminal(first);
    poll_until(
        &waiter,
        "session 1 to leave the registry after close_terminal",
        || (core.terminal_frame(first).is_none(), core.live_terminals()),
        |(gone, live)| *gone && *live == vec![second],
    );

    let third = open_session(&core, SPLIT_CURRENT, sleeper());
    assert_eq!(
        third,
        TerminalId(3),
        "the id after 1 and 2 must be 3 even though 1 is free again; {third:?} means a freed id \
         was reused, so a front-end could bind a stale pane to a new session."
    );
    assert_eq!(
        core.live_terminals(),
        vec![second, third],
        "live_terminals must list the running sessions in ascending order."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A program that cannot be started opens nothing: `None`, no placeholder bound to a session
/// and no session in the registry.
#[test]
fn a_missing_program_opens_nothing() {
    let (core, waiter, _cwd) = start_core();
    let missing = TerminalCommand {
        program: PathBuf::from("/nonexistent/wolf-shell"),
        args: Vec::new(),
    };

    let opened = core.open_terminal(TerminalTarget::Window(0), Some(missing));
    assert_eq!(
        opened, None,
        "open_terminal with a program that does not exist must return None, not {opened:?}."
    );
    settle_for(&waiter, QUIET_SETTLE);
    assert_eq!(
        terminal_windows(&core),
        Vec::new(),
        "no window may carry a session after a failed open; one does when the placeholder is \
         bound before the spawn and kept when the spawn fails."
    );
    assert_eq!(
        core.live_terminals(),
        Vec::new(),
        "no session may be registered after a failed spawn."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `open_terminal(Window(0))` binds the session to the current window: its `WinBuffer`
/// carries the id and no path, the path trackers do not see it, it is full width, and its
/// buffer is the unlisted, unnamed, unmodifiable `nofile` placeholder the spec describes.
#[test]
fn the_placeholder_is_bound_in_win_buffers() {
    let (core, waiter, cwd) = start_core();
    let win = active_window(&core, &waiter);
    let id = open_session(
        &core,
        TerminalTarget::Window(0),
        sh_script("exec sleep 1000"),
    );

    // D4: the bind ends with wolf_sync_buffers, so the binding is in the snapshot on return.
    let bound = core.read_snapshot().win_buffers.get(&win).cloned();
    let bound = bound.unwrap_or_else(|| {
        panic!("window {win} has no win_buffers entry right after open_terminal returned")
    });
    assert_eq!(
        bound.terminal,
        Some(id),
        "right after open_terminal returns, window {win}'s WinBuffer must carry {id:?}, not {:?}. \
         None means init.lua sends no id, core ignores the sixth element of wolf_buffers, or \
         open_terminal returns before the binding is applied.",
        bound.terminal
    );
    assert_eq!(
        bound.path, None,
        "the placeholder is unnamed, so its WinBuffer must have no path."
    );
    assert!(
        !core.read_snapshot().buffers.contains_key(&win),
        "the path tracker `buffers` must not list the placeholder's window {win}."
    );
    poll_until(
        &waiter,
        "the placeholder's window to be full width",
        || core.read_snapshot().full_width.get(&win).copied(),
        |full| *full == Some(true),
    );

    let probe = cwd.join("placeholder-probe.txt");
    let buf = format!("winbufnr({win})");
    let exprs = [
        format!("getbufvar({buf}, '&buftype')"),
        format!("getbufvar({buf}, '&modifiable')"),
        format!("getbufvar({buf}, '&buflisted')"),
        format!("getbufvar({buf}, '&bufhidden')"),
        format!("getbufvar({buf}, '&swapfile')"),
        format!("getbufvar({buf}, 'wolf_full_width') ? 1 : 0"),
        format!("getbufvar({buf}, 'wolf_terminal_id')"),
        format!("'name:' . bufname({buf})"),
    ];
    let exprs: Vec<&str> = exprs.iter().map(String::as_str).collect();
    let values = probe_values(&core, &exprs, &probe);
    assert_eq!(
        values,
        vec![
            "nofile".to_owned(),
            "0".to_owned(),
            "0".to_owned(),
            "wipe".to_owned(),
            "0".to_owned(),
            "1".to_owned(),
            id.0.to_string(),
            "name:".to_owned(),
        ],
        "the placeholder buffer must read (buftype, modifiable, buflisted, bufhidden, swapfile, \
         b:wolf_full_width, b:wolf_terminal_id, name) as nofile, 0, 0, wipe, 0, 1, {}, empty. A \
         bufhidden other than wipe leaves the buffer alive when its window closes, so the session \
         is never hung up.",
        id.0
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Typing `:terminal`, `:term` or `:ter` opens a core session in the current window: the
/// window's `WinBuffer` carries a new session, its buffer is the `nofile` placeholder rather
/// than Neovim's own terminal, and a line typed into the session runs in a shell.
#[test]
fn typed_terminal_words_open_a_session_here() {
    common::pin_default_shell();
    for word in ["terminal", "term", "ter"] {
        let (core, waiter, cwd) = start_core();
        let win = active_window(&core, &waiter);
        let before = core
            .read_snapshot()
            .win_buffers
            .get(&win)
            .map(|buffer| buffer.bufnr);

        send_ex(&core, word);
        poll_until(
            &waiter,
            &format!("typed `:{word}` to change window {win}'s buffer"),
            || core.read_snapshot().win_buffers.get(&win).cloned(),
            |buffer| {
                buffer
                    .as_ref()
                    .is_some_and(|buffer| buffer.terminal.is_some() || Some(buffer.bufnr) != before)
            },
        );
        // A changed bufnr may be seen before the binding lands; let it settle, then read again.
        settle_for(&waiter, QUIET_SETTLE);
        let after = core
            .read_snapshot()
            .win_buffers
            .get(&win)
            .cloned()
            .unwrap_or_else(|| panic!("window {win} left win_buffers after typed `:{word}`"));

        let probe = cwd.join("typed-terminal-probe.txt");
        let buftype = format!("getbufvar(winbufnr({win}), '&buftype')");
        let values = probe_values(&core, &[buftype.as_str()], &probe);
        assert_eq!(
            (after.terminal.is_some(), values[0].as_str()),
            (true, "nofile"),
            "typed `:{word}` must open a core session in the current window {win}: its \
             WinBuffer must carry a session and its buffer be the nofile placeholder. (false, \
             \"terminal\") means init.lua has no `{word}` abbreviation, so Neovim's own \
             :terminal ran; (false, \"\") means the abbreviation ran but core opened no \
             session (no `wolf_terminal_open` handler)."
        );
        let id = after
            .terminal
            .expect("the assertion above checked the window carries a session");
        assert_eq!(
            core.live_terminals(),
            vec![id],
            "typed `:{word}` must start exactly one session, the one window {win} shows."
        );

        run_line(&core, id, "echo typed-$((4+5))");
        wait_for_text(&core, &waiter, id, "typed-9");

        core.shutdown().expect("shutdown should be Ok");
    }
}

/// `pane_status` of a terminal pane's grid reports the session's shell name and empty paths.
#[test]
fn pane_status_reports_the_shell_name() {
    let (core, waiter, _cwd) = start_core();
    let win = active_window(&core, &waiter);
    let _id = open_session(
        &core,
        TerminalTarget::Window(0),
        sh_script("exec sleep 1000"),
    );
    let grid = poll_until(
        &waiter,
        "a grid showing the placeholder's window",
        || grid_of_win(&core, win),
        Option::is_some,
    )
    .expect("poll_until returned only once the grid was found");

    let status = core.pane_status(grid);
    assert_eq!(
        (
            status.filename.as_str(),
            status.display_path.as_str(),
            status.abs_path.as_str()
        ),
        ("sh", "", ""),
        "a terminal pane's status must be the shell's name with empty paths, not {status:?}. \
         {NO_NAME:?} means pane_status read the placeholder's buffer name instead of the \
         session."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Each row's version is the frame version at which the row last changed: a written row moves
/// past the previous frame's version, an untouched row keeps its version, and after a resize
/// every row carries the resize frame's version.
#[test]
fn row_versions_follow_the_rows_that_change() {
    let (core, waiter, _cwd) = start_core();
    // The cursor rests on row 11 between steps, so rows 2, 7 and 20 are written or untouched
    // but never crossed by it.
    let script = "stty -echo; printf ready; read x; printf '\\033[3;1HAAA\\033[12;1H'; read y; \
                  printf '\\033[8;1HCCC\\033[12;1H'; exec sleep 1000";
    let id = open_session(&core, TerminalTarget::Window(0), sh_script(script));
    wait_for_text(&core, &waiter, id, "ready");
    settle_for(&waiter, QUIET_SETTLE);
    let f0 = frame_of(&core, id);
    assert_eq!(
        f0.row_versions.len(),
        usize::from(f0.rows),
        "row_versions must hold one version per row."
    );

    core.terminal_key(id, term_key(Key::Enter));
    let f1 = poll_frame(&core, &waiter, id, "AAA on row 2", |frame| {
        frame.row_text(2) == "AAA" && frame.cursor.as_ref().is_some_and(|cursor| cursor.row == 11)
    });
    assert!(
        f1.row_versions[2] > f0.version && f1.row_versions[2] <= f1.version,
        "row 2 was written after frame {}, so its version must be above it and at most the \
         current {}, not {}.",
        f0.version,
        f1.version,
        f1.row_versions[2]
    );
    assert!(
        f1.row_versions[7] <= f0.version && f1.row_versions[20] <= f0.version,
        "rows 7 and 20 were not touched after frame {}, so their versions must not pass it, not \
         {} and {}: every row is stamped on every publish.",
        f0.version,
        f1.row_versions[7],
        f1.row_versions[20]
    );

    core.terminal_key(id, term_key(Key::Enter));
    let f2 = poll_frame(&core, &waiter, id, "CCC on row 7", |frame| {
        frame.row_text(7) == "CCC" && frame.cursor.as_ref().is_some_and(|cursor| cursor.row == 11)
    });
    assert!(
        f2.row_versions[7] > f1.version && f2.row_versions[7] <= f2.version,
        "row 7 was written after frame {}, so its version must be above it and at most {}, not \
         {}.",
        f1.version,
        f2.version,
        f2.row_versions[7]
    );
    assert_eq!(
        (f2.row_versions[2], f2.row_versions[20]),
        (f1.row_versions[2], f1.row_versions[20]),
        "rows 2 and 20 did not change while row 7 was written, so they must keep their versions."
    );

    core.resize_terminal(id, 90, 30, 8, 16);
    let f3 = poll_frame(&core, &waiter, id, "the frame to be 90x30", |frame| {
        (frame.cols, frame.rows) == (90, 30)
    });
    assert_eq!(
        f3.row_versions.len(),
        30,
        "after growing to 30 rows, row_versions must hold 30 versions."
    );
    let stale: Vec<(usize, u64)> = f3
        .row_versions
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, version)| *version != f3.version)
        .collect();
    assert!(
        stale.is_empty(),
        "a resize redraws every row, so every row's version must be the frame's {}; these rows \
         (row, version) are not: {stale:?}. A full redraw is being treated as a partial one.",
        f3.version
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A publish advances the frame's version and fires the core's wake, and never bumps the
/// snapshot's `seq`.
#[test]
fn a_publish_wakes_without_bumping_seq() {
    let cwd = std::fs::canonicalize(common::fresh_cwd()).expect("failed to canonicalize the cwd");
    let (core, waiter, wakes) = start_core_recording(cwd);
    quiet_neovim(&core, &waiter);
    let id = open_session(
        &core,
        TerminalTarget::Window(0),
        sh_script("stty -echo; printf ready; read x; echo out-$((3+4)); exec sleep 1000"),
    );
    wait_for_text(&core, &waiter, id, "ready");
    settle_for(&waiter, QUIET_SETTLE);

    let seq_before = core.read_snapshot().seq;
    let version_before = frame_of(&core, id).version;
    wakes.clear();
    core.terminal_key(id, term_key(Key::Enter));
    let frame = wait_for_text(&core, &waiter, id, "out-7");

    assert!(
        frame.version > version_before,
        "new output must advance the frame's version past {version_before}, not leave it at {}.",
        frame.version
    );
    assert!(
        !wakes.instants().is_empty(),
        "the publish of new output must fire the core's wake; none fired, so a front-end would \
         not repaint until something else woke it."
    );
    assert_eq!(
        core.read_snapshot().seq,
        seq_before,
        "a terminal publish must not bump Snapshot.seq, which only a redraw flush may; it moved \
         from {seq_before}."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A flood of output publishes many frames, never two within `TERMINAL_PUBLISH_INTERVAL`.
#[test]
fn publishes_are_coalesced() {
    let cwd = std::fs::canonicalize(common::fresh_cwd()).expect("failed to canonicalize the cwd");
    let (core, waiter, wakes) = start_core_recording(cwd);
    quiet_neovim(&core, &waiter);
    let script = "stty -echo; printf ready; read x; i=0; \
                  while [ $i -lt 20000 ]; do echo line $i; i=$((i+1)); done; exec sleep 1000";
    let id = open_session(&core, TerminalTarget::Window(0), sh_script(script));
    wait_for_text(&core, &waiter, id, "ready");
    settle_for(&waiter, QUIET_SETTLE);

    let seq_before = core.read_snapshot().seq;
    wakes.clear();
    core.terminal_key(id, term_key(Key::Enter));
    wait_for_text(&core, &waiter, id, "line 19999");
    let instants = wakes.instants();

    assert_eq!(
        core.read_snapshot().seq,
        seq_before,
        "Neovim redrew during the flood, so its wakes are mixed into the ones measured and the \
         gaps below prove nothing."
    );
    assert!(
        instants.len() >= 4,
        "a flood of 20000 lines must publish several frames, not {}: the reader publishes only \
         at the end, or publishing does not wake.",
        instants.len()
    );
    let floor = TERMINAL_PUBLISH_INTERVAL.saturating_sub(Duration::from_millis(1));
    let short: Vec<Duration> = instants
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .filter(|gap| *gap < floor)
        .collect();
    assert!(
        short.is_empty(),
        "consecutive publishes must be at least {TERMINAL_PUBLISH_INTERVAL:?} apart (less 1 ms \
         of timer slack); {} of {} gaps were shorter: {short:?}. Publishing is not coalesced.",
        short.len(),
        instants.len() - 1
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Output inside a synchronized update (mode 2026) is not published until the program ends
/// the update, and is published promptly once it does.
#[test]
fn a_render_hold_defers_publishing_until_released() {
    let (core, waiter, _cwd) = start_core();
    let script = "stty -echo; printf 'pre\\r\\n'; sleep 0.2; printf '\\033[?2026hHELD'; read x; \
                  printf '\\033[?2026l'; exec sleep 1000";
    let id = open_session(&core, TerminalTarget::Window(0), sh_script(script));
    wait_for_text(&core, &waiter, id, "pre");

    // The hold starts about 0.2 s after `pre`; unheld, HELD would show soon after.
    let watch_until = Instant::now() + TERMINAL_RENDER_HOLD_MAX / 2;
    while Instant::now() < watch_until {
        let frame = frame_of(&core, id);
        assert!(
            !frame_text(&frame).contains("HELD"),
            "HELD was published while the program held rendering with mode 2026; RenderHold is \
             ignored. The frame reads:\n{}",
            frame_text(&frame)
        );
        std::thread::sleep(POLL_INTERVAL);
    }

    let released = Instant::now();
    core.terminal_key(id, term_key(Key::Enter));
    wait_for_text(&core, &waiter, id, "HELD");
    let took = released.elapsed();
    assert!(
        took <= TERMINAL_RENDER_HOLD_MAX / 2,
        "HELD must be published promptly once the program resets mode 2026, but took {took:?}: \
         the release is ignored and only the maximum ended the hold."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A synchronized update the program never ends is published once
/// `TERMINAL_RENDER_HOLD_MAX` has passed, and not before.
#[test]
fn an_unreleased_render_hold_ends_at_the_maximum() {
    let (core, waiter, _cwd) = start_core();
    let script = "printf 'pre\\r\\n'; sleep 0.2; printf '\\033[?2026hHELD'; exec sleep 1000";
    let id = open_session(&core, TerminalTarget::Window(0), sh_script(script));
    wait_for_text(&core, &waiter, id, "pre");
    let pre_seen = Instant::now();

    let deadline = pre_seen + TERMINAL_RENDER_HOLD_MAX + Duration::from_secs(2);
    let held_seen = loop {
        if frame_text(&frame_of(&core, id)).contains("HELD") {
            break Instant::now();
        }
        assert!(
            Instant::now() < deadline,
            "HELD was never published although the hold began more than \
             {TERMINAL_RENDER_HOLD_MAX:?} ago: the hold has no maximum."
        );
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    };
    let after = held_seen - pre_seen;
    assert!(
        after >= TERMINAL_RENDER_HOLD_MAX.mul_f64(0.9),
        "HELD was published {after:?} after `pre`, but the hold began about 0.2 s after `pre` and \
         lasts {TERMINAL_RENDER_HOLD_MAX:?}: the hold is ignored or ends early."
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// The default colours follow the editor's `Normal`, the ANSI colours follow
/// `g:terminal_color_N`, and the cursor colour follows `WolfCursor`, for a running session.
#[test]
fn colours_follow_the_editor_theme() {
    let (core, waiter, _cwd) = start_core();
    let id = open_session(
        &core,
        TerminalTarget::Window(0),
        sh_script("printf '\\033[31mR\\033[0m'; exec sleep 1000"),
    );
    wait_for_text(&core, &waiter, id, "R");

    let fg = Rgb {
        r: 0x01,
        g: 0x02,
        b: 0x03,
    };
    let bg = Rgb {
        r: 0x04,
        g: 0x05,
        b: 0x06,
    };
    send_ex(&core, "hi Normal guifg=#010203 guibg=#040506");
    poll_frame(
        &core,
        &waiter,
        id,
        "default_fg #010203 and default_bg #040506 after `:hi Normal`; the old colours mean core \
         does not push the editor's defaults after a batch that set them",
        |frame| frame.default_fg == fg && frame.default_bg == bg,
    );

    let ansi_red = Rgb {
        r: 0x12,
        g: 0x34,
        b: 0x56,
    };
    let red_cell = |core: &WolfCore| frame_of(core, id).cell(0, 0).and_then(|cell| cell.fg);
    assert_ne!(
        red_cell(&core),
        Some(ansi_red),
        "precondition: the SGR 31 cell must not already have the test colour."
    );
    send_ex(&core, "let g:terminal_color_1 = '#123456'");
    send_ex(&core, "doautocmd ColorScheme");
    poll_until(
        &waiter,
        "the SGR 31 cell's foreground to become g:terminal_color_1 (#123456); unchanged means \
         wolf_theme's ANSI colours are not read or not pushed to the session's palette",
        || red_cell(&core),
        |colour| *colour == Some(ansi_red),
    );

    let cursor = Rgb {
        r: 0x0a,
        g: 0x0b,
        b: 0x0c,
    };
    send_ex(&core, "hi WolfCursor guibg=#0a0b0c");
    send_ex(&core, "doautocmd ColorScheme");
    poll_frame(
        &core,
        &waiter,
        id,
        "cursor_color #0a0b0c after `:hi WolfCursor`; anything else means the cursor colour is \
         not pushed to the session",
        |frame| frame.cursor_color == Some(cursor),
    );

    core.shutdown().expect("shutdown should be Ok");
}
