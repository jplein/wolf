//! Integration tests of the `wolf-ffi` terminal-session bridge: `WolfCoreHandle`'s
//! `open_terminal`, `bind_terminal`, `terminal_frame`, the input calls, `resize_terminal`,
//! `close_terminal`, `live_terminals` and `terminal_selection_text`, the opaque
//! `FfiTerminalFrame` and `FfiTerminalCell` read path, and `FfiWinBuffer::terminal`.
//!
//! Each test starts a real, hermetic Neovim through `wolf_ffi_start` and a real `/bin/sh` in
//! a pty, with no mocks, reusing `tests/common/mod.rs`. Core's own behaviour is proved by
//! `crates/wolf-core/tests/terminal_*.rs`; these tests prove the bridge carries it: every
//! discriminant, bit and packed colour, and the empty answers of a shut-down handle.
//!
//! ## Premises about the components
//!
//! These are measured against the real components by the core and `wolf-vt` suites, and
//! re-observed here by the assertions that depend on them:
//!
//! - A command other than `""` runs as `/bin/sh -c <command>`, so the session's shell name,
//!   which `pane_status` reports, is `sh`.
//! - `/bin/sh` with no argument on a tty is interactive and echoes what it is typed, so a
//!   test looks for the output of an expression (`$((6*7))`) the typed text does not contain.
//! - Under `stty raw -echo`, `cat -v` prints each byte it is sent as it arrives, ESC as `^[`.
//! - `CSI 5 SP q` selects a blinking bar cursor, SGR `4:3` a curly underline, and a wide
//!   character takes a wide cell followed by a spacer-tail cell.
//! - A session starts at 80x24 with a cell of 8x16 pixels; core maps a pointer position to
//!   the cell `floor(px / cell_px)` of the last `resize_terminal`.
//!
//! ## SKIP-when-unset policy
//!
//! Loudly SKIPS (no-op pass) when no pinned nvim is available, via
//! `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent, FfiTerminalFrame, WolfCoreHandle};

/// How long any effect may take to surface: nvim startup, a shell's output, a publish.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// How long bytes that should not arrive get to arrive before a negative check.
const NEGATIVE_SETTLE: Duration = Duration::from_millis(500);

/// `terminal_target::WINDOW`: show the session in the given window.
const TARGET_WINDOW: u8 = 0;
/// `terminal_target::SPLIT_VERTICAL`: show the session in a vertical split of the window.
const TARGET_SPLIT_VERTICAL: u8 = 2;
/// A target no `terminal_target` constant names.
const TARGET_UNKNOWN: u8 = 9;

/// `terminal_mouse_kind::PRESS`.
const MOUSE_PRESS: u8 = 0;
/// `terminal_mouse_kind::RELEASE`.
const MOUSE_RELEASE: u8 = 1;
/// `terminal_mouse_kind::MOTION`.
const MOUSE_MOTION: u8 = 2;
/// `mouse_button::LEFT`.
const BUTTON_LEFT: u8 = 0;
/// `scroll_dir::UP`.
const SCROLL_UP: u8 = 0;
/// No `terminal_mods` bit.
const NO_MODS: u8 = 0;

/// `terminal_cursor_style::BAR`.
const CURSOR_BAR: u8 = 1;
/// `terminal_underline::CURLY`.
const UNDERLINE_CURLY: u8 = 3;
/// `terminal_cell_width::WIDE`.
const WIDTH_WIDE: u8 = 1;
/// `terminal_cell_width::SPACER_TAIL`.
const WIDTH_SPACER_TAIL: u8 = 2;
/// `terminal_flag::BOLD`.
const FLAG_BOLD: u8 = 1 << 0;
/// `terminal_flag::ITALIC`.
const FLAG_ITALIC: u8 = 1 << 2;

/// `kind::ENTER`, the `FfiKeyEvent.kind` of the Return key.
const KIND_ENTER: u8 = 1;
/// `intent_kind::SPLIT`.
const INTENT_SPLIT: u8 = 0;
/// `orientation::VERTICAL`, panes side by side.
const ORIENTATION_VERTICAL: u8 = 1;

/// The marker a [`cat_command`] session prints once its tty is raw, before `cat` reads.
const CAT_READY: &str = "cat-ready";

/// Polls `pred` until it holds or [`TIMEOUT`] passes; panics with `desc` on timeout.
fn poll(desc: &str, mut pred: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        if pred() {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out after {TIMEOUT:?} waiting for: {desc}");
}

/// Polls session `id`'s frame until `pred` accepts it or `timeout` passes, returning the
/// accepted frame as `Ok` and the last frame read as `Err`.
fn poll_frame(
    handle: &WolfCoreHandle,
    id: u64,
    timeout: Duration,
    pred: impl Fn(&FfiTerminalFrame) -> bool,
) -> Result<FfiTerminalFrame, FfiTerminalFrame> {
    let deadline = Instant::now() + timeout;
    loop {
        let frame = handle.terminal_frame(id);
        if pred(&frame) {
            return Ok(frame);
        }
        if Instant::now() >= deadline {
            return Err(frame);
        }
        sleep(POLL_INTERVAL);
    }
}

/// [`poll_frame`] over [`TIMEOUT`], panicking with `desc` and the last frame's text.
fn wait_frame(
    handle: &WolfCoreHandle,
    id: u64,
    desc: &str,
    pred: impl Fn(&FfiTerminalFrame) -> bool,
) -> FfiTerminalFrame {
    poll_frame(handle, id, TIMEOUT, pred).unwrap_or_else(|last| {
        panic!(
            "timed out after {TIMEOUT:?} waiting for {desc}; session {id}'s frame (version {}, \
             {}x{}, exited {}) reads:\n{}",
            last.version(),
            last.cols(),
            last.rows(),
            last.has_exited(),
            frame_text(&last)
        )
    })
}

/// Row `row`'s text from `row_cells`, an empty cell as one space, trailing spaces trimmed.
fn row_text(frame: &FfiTerminalFrame, row: u16) -> String {
    let mut text: String = frame
        .row_cells(row)
        .iter()
        .map(|cell| {
            let t = cell.text();
            if t.is_empty() {
                " ".to_owned()
            } else {
                t
            }
        })
        .collect();
    text.truncate(text.trim_end_matches(' ').len());
    text
}

/// Every row's text, joined with newlines.
fn frame_text(frame: &FfiTerminalFrame) -> String {
    (0..frame.rows())
        .map(|row| row_text(frame, row))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether some row of `frame` reads exactly `line`.
fn has_row(frame: &FfiTerminalFrame, line: &str) -> bool {
    (0..frame.rows()).any(|row| row_text(frame, row) == line)
}

/// A byte-inspection session's command: `prelude` is printed (a `printf` format, so `\033`
/// is ESC), the tty goes raw without echo, [`CAT_READY`] is printed, and `cat -v` shows
/// every byte the session is sent.
fn cat_command(prelude: &str) -> String {
    format!("printf '{prelude}'; stty raw -echo; printf '{CAT_READY}\\r\\n'; exec cat -v")
}

/// What a [`cat_command`] session's `cat -v` printed: the rows after the [`CAT_READY`] row,
/// joined with nothing between them, trimmed. Empty until the marker shows.
fn cat_output(frame: &FfiTerminalFrame) -> String {
    let rows: Vec<String> = (0..frame.rows()).map(|row| row_text(frame, row)).collect();
    match rows.iter().position(|row| row.contains(CAT_READY)) {
        Some(marker) => rows[marker + 1..].concat().trim().to_owned(),
        None => String::new(),
    }
}

/// Waits until a cat session shows output, lets the rest arrive, and returns all of it.
fn cat_shows(handle: &WolfCoreHandle, id: u64, desc: &str) -> String {
    wait_frame(handle, id, desc, |frame| !cat_output(frame).is_empty());
    sleep(NEGATIVE_SETTLE);
    cat_output(&handle.terminal_frame(id))
}

/// Starts a core in a fresh cwd and waits for it to attach, or `None` to skip.
fn start() -> Option<WolfCoreHandle> {
    common::nvim_bin_or_skip()?;
    let init_lua = common::init_lua_path();
    let cwd = common::fresh_cwd();
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        cwd.to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");
    poll("the core to attach (read_frame cols > 0)", || {
        handle.read_frame().cols() > 0
    });
    Some(handle)
}

/// Waits for the snapshot to name an active window and returns it.
fn active_window(handle: &WolfCoreHandle) -> i64 {
    poll("the snapshot to name an active window", || {
        handle.active_win() != 0
    });
    handle.active_win()
}

/// Opens `command` in `target` of `win`, failing on a 0 id.
fn open(handle: &WolfCoreHandle, win: i64, target: u8, command: &str) -> u64 {
    let id = handle.open_terminal(win, target, command.to_owned());
    assert_ne!(
        id, 0,
        "open_terminal({win}, {target}, {command:?}) returned 0: the pty or the shell could not \
         be started, the placeholder could not be bound, or the target was not mapped"
    );
    id
}

/// A key event of `kind` with no modifier.
fn named_key(kind: u8) -> FfiKeyEvent {
    FfiKeyEvent {
        kind,
        ch: 0,
        fnum: 0,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    }
}

/// A session opened into the current window binds that window's `FfiWinBuffer::terminal`,
/// reports the shell name as the pane's status, and publishes its styled cells through the
/// flat per-row accessors.
#[test]
fn open_terminal_binds_the_window_and_publishes_styled_cells() {
    let Some(handle) = start() else {
        return;
    };
    let win = active_window(&handle);
    let id = open(
        &handle,
        0,
        TARGET_WINDOW,
        "printf '\\033[1;3;38;2;255;0;0;48;2;0;0;255mAB\\033[0m'; exec sleep 600",
    );

    assert!(
        handle.live_terminals().contains(&id),
        "a session open_terminal just returned must be in live_terminals(), which reads {:?}",
        handle.live_terminals()
    );
    poll(
        "win_buffer(active window).terminal() to name the new session",
        || handle.win_buffer(win).terminal() == id,
    );

    let frame = wait_frame(&handle, id, "row 0 to start with `AB`", |frame| {
        row_text(frame, 0).starts_with("AB")
    });
    let cells = frame.row_cells(0);
    let a = cells
        .first()
        .expect("row_cells(0) must hold the row's cells, not be empty");
    assert_eq!(a.text(), "A", "row 0 cell 0 must be the printed `A`");
    let (fg, bg) = (a.fg(), a.bg());
    assert_eq!(
        (fg.rgb, fg.set),
        (0x00FF_0000, true),
        "cell 0's fg must be the SGR 38;2;255;0;0 red, set. 0x0000FF means fg() packs the bg; \
         unset means the colour was dropped"
    );
    assert_eq!(
        (bg.rgb, bg.set),
        (0x0000_00FF, true),
        "cell 0's bg must be the SGR 48;2;0;0;255 blue, set. 0xFF0000 means bg() packs the fg"
    );
    assert_eq!(
        a.flags(),
        FLAG_BOLD | FLAG_ITALIC,
        "cell 0 after SGR 1;3 must carry exactly BOLD | ITALIC ({:#04b}), not {:#04b}: a missing \
         bit drops a face, an extra one paints an attribute the program never set",
        FLAG_BOLD | FLAG_ITALIC,
        a.flags()
    );
    assert!(
        frame.row_version(0) > 0,
        "row_version(0) must be past 0 once row 0 shows output; 0 is the empty frame's value"
    );

    poll("a grid showing the placeholder's window", || {
        handle.grid_for_win(win) != 0
    });
    let status = handle.pane_status_for_grid(handle.grid_for_win(win));
    assert_eq!(
        (status.filename(), status.abs_path()),
        ("sh".to_owned(), String::new()),
        "a terminal pane's status must name the session's shell, `sh` for a `/bin/sh -c` \
         command, with an empty path. `[No Name]` means the bridge read the placeholder buffer"
    );

    handle.shutdown();
}

/// Text and keys sent through the bridge reach the shell, which runs the typed line.
#[test]
fn text_and_keys_reach_the_shell() {
    let Some(handle) = start() else {
        return;
    };
    let id = open(&handle, 0, TARGET_WINDOW, "exec /bin/sh");

    handle.terminal_text(id, "echo wolf$((6*7))".to_owned());
    handle.terminal_key(id, named_key(KIND_ENTER));
    let shown = poll_frame(&handle, id, TIMEOUT, |frame| has_row(frame, "wolf42"));
    assert!(
        shown.is_ok(),
        "after typing `echo wolf$((6*7))` and Enter, a row must read `wolf42`. The typed line \
         alone means terminal_key dropped Enter; nothing means terminal_text dropped the text. \
         The frame reads:\n{}",
        frame_text(&shown.unwrap_or_else(|last| last))
    );

    handle.shutdown();
}

/// `resize_terminal` sizes the frame and the pty: the frame is 100x30 and `stty size` in the
/// shell prints `30 100`.
#[test]
fn resize_sizes_the_frame_and_the_pty() {
    let Some(handle) = start() else {
        return;
    };
    let id = open(&handle, 0, TARGET_WINDOW, "exec /bin/sh");

    handle.resize_terminal(id, 100, 30, 9, 18);
    let sized = poll_frame(&handle, id, TIMEOUT, |frame| frame.cols() != 80);
    let frame = sized.unwrap_or_else(|last| last);
    assert_eq!(
        (frame.cols(), frame.rows()),
        (100, 30),
        "resize_terminal(100, 30) must make the frame 100 columns by 30 rows. 30x100 means cols \
         and rows are swapped on the way to core; 80x24 means the resize never reached it"
    );

    handle.terminal_text(id, "stty size\r".to_owned());
    let printed = poll_frame(&handle, id, TIMEOUT, |frame| has_row(frame, "30 100"));
    assert!(
        printed.is_ok(),
        "`stty size` must print `30 100` once the session is 100x30; `24 80` means the pty was \
         not resized. The frame reads:\n{}",
        frame_text(&printed.unwrap_or_else(|last| last))
    );

    handle.shutdown();
}

/// `terminal_paste` goes through the paste encoder, which brackets the text for a program
/// that set mode 2004.
#[test]
fn paste_is_bracketed_when_the_program_asks() {
    let Some(handle) = start() else {
        return;
    };
    let id = open(&handle, 0, TARGET_WINDOW, &cat_command("\\033[?2004h"));
    wait_frame(&handle, id, "the cat session's ready marker", |frame| {
        frame_text(frame).contains(CAT_READY)
    });

    handle.terminal_paste(id, "hi".to_owned());
    let output = cat_shows(&handle, id, "the bytes of a paste");
    assert_eq!(
        output, "^[[200~hi^[[201~",
        "a paste into a program that set mode 2004 must arrive bracketed, not as {output:?}: a \
         bare `hi` means terminal_paste went through the text path, not the paste encoder"
    );

    handle.shutdown();
}

/// A left drag through `terminal_mouse` selects the cells under it, and the selection reads
/// back through `terminal_selection_text` and the per-row selection accessors.
#[test]
fn a_drag_selects_and_the_selection_reads_back() {
    let Some(handle) = start() else {
        return;
    };
    let id = open(
        &handle,
        0,
        TARGET_WINDOW,
        "printf 'WOLFTERM'; exec sleep 600",
    );
    wait_frame(&handle, id, "row 0 to read WOLFTERM", |frame| {
        row_text(frame, 0) == "WOLFTERM"
    });
    // 81 columns, not 80, so the frame shows when the resize, and with it the 10x20 cell, applied.
    handle.resize_terminal(id, 81, 24, 10, 20);
    wait_frame(&handle, id, "the frame to take 81 columns", |frame| {
        frame.cols() == 81
    });

    // Pixel x 5 is in column 0 and x 35 in column 3, both in row 0 at y 10.
    handle.terminal_mouse(id, MOUSE_PRESS, BUTTON_LEFT, 5.0, 10.0, NO_MODS);
    handle.terminal_mouse(id, MOUSE_MOTION, BUTTON_LEFT, 35.0, 10.0, NO_MODS);
    handle.terminal_mouse(id, MOUSE_RELEASE, BUTTON_LEFT, 35.0, 10.0, NO_MODS);
    let selected = poll_frame(&handle, id, TIMEOUT, |frame| frame.row_has_selection(0));
    let frame = selected.unwrap_or_else(|last| last);
    assert!(
        frame.row_has_selection(0),
        "a drag across row 0 must select in row 0; none means MOTION is not mapped to a drag \
         or row_has_selection does not read the frame's selection"
    );
    assert_eq!(
        (frame.row_selection_start(0), frame.row_selection_end(0)),
        (0, 3),
        "a drag from x 5 to x 35 with 10-pixel cells must select columns 0 to 3 inclusive. An \
         end equal to the start means row_selection_end reads the start"
    );
    assert!(
        !frame.row_has_selection(1),
        "the drag stayed in row 0, so row 1 must have no selection"
    );
    assert_eq!(
        handle.terminal_selection_text(id),
        "WOLF",
        "the selection over columns 0 to 3 must copy as `WOLF`. `W` or \"\" means the motion \
         ended the drag (MOTION mapped to RELEASE)"
    );

    handle.shutdown();
}

/// On the primary screen with tracking off, `terminal_scroll` up moves the viewport up
/// through scrollback, which the scrollbar accessors report.
#[test]
fn wheel_scrolls_the_viewport_through_scrollback() {
    let Some(handle) = start() else {
        return;
    };
    let id = open(
        &handle,
        0,
        TARGET_WINDOW,
        "i=1; while [ $i -le 200 ]; do echo L$i; i=$((i+1)); done; exec sleep 600",
    );
    let bottom = wait_frame(&handle, id, "L200 on screen", |frame| {
        has_row(frame, "L200")
    });
    assert!(
        bottom.scrollbar_total() > bottom.scrollbar_len(),
        "200 lines in 24 rows must leave scrollback: total {} must exceed len {}",
        bottom.scrollbar_total(),
        bottom.scrollbar_len()
    );
    let at_bottom = bottom.scrollbar_total() - bottom.scrollbar_len();
    assert_eq!(
        bottom.scrollbar_offset(),
        at_bottom,
        "before any scroll the viewport must be at the bottom, offset total - len"
    );

    handle.terminal_scroll(id, SCROLL_UP, 5, 0.0, 0.0, NO_MODS);
    let scrolled = poll_frame(&handle, id, TIMEOUT, |frame| {
        frame.scrollbar_offset() != at_bottom
    });
    let frame = scrolled.unwrap_or_else(|last| last);
    assert_eq!(
        frame.scrollbar_offset(),
        at_bottom - 5,
        "a wheel scroll up of 5 lines must lower the offset by 5. Unchanged means the scroll was \
         sent down (scroll_dir::UP mapped to Down) or dropped"
    );

    handle.shutdown();
}

/// `terminal_focus(id, true)` reaches a program that set mode 1004 as a focus-in report.
#[test]
fn focus_reports_reach_a_program_that_asked() {
    let Some(handle) = start() else {
        return;
    };
    let id = open(&handle, 0, TARGET_WINDOW, &cat_command("\\033[?1004h"));
    wait_frame(&handle, id, "the cat session's ready marker", |frame| {
        frame_text(frame).contains(CAT_READY)
    });

    handle.terminal_focus(id, true);
    let output = cat_shows(&handle, id, "a focus report");
    assert_eq!(
        output, "^[[I",
        "focus gained with mode 1004 set must send CSI I, not {output:?}: ^[[O means the bridge \
         inverted `gained`"
    );

    handle.shutdown();
}

/// The cursor style, blink, underline style and cell width cross the bridge as their
/// discriminants.
#[test]
fn cursor_style_underline_and_width_are_packed() {
    let Some(handle) = start() else {
        return;
    };
    let id = open(
        &handle,
        0,
        TARGET_WINDOW,
        "printf '\\033[5 q\\033[4:3mU\\033[0m世'; exec sleep 600",
    );
    let frame = wait_frame(&handle, id, "row 0 to show `U` then `世`", |frame| {
        frame
            .row_cells(0)
            .get(1)
            .is_some_and(|cell| cell.text() == "世")
    });

    assert!(
        frame.has_cursor() && frame.cursor_visible(),
        "the cursor must be present and visible after printing"
    );
    assert_eq!(
        frame.cursor_style(),
        CURSOR_BAR,
        "CSI 5 SP q must give cursor_style BAR (1): 0 means Bar maps to BLOCK"
    );
    assert!(
        frame.cursor_blinking(),
        "CSI 5 SP q selects a blinking bar, so cursor_blinking must be true"
    );
    let cells = frame.row_cells(0);
    assert_eq!(
        cells[0].underline(),
        UNDERLINE_CURLY,
        "`U` after SGR 4:3 must have underline CURLY (3): 1 means Curly maps to SINGLE"
    );
    assert_eq!(
        cells[1].width(),
        WIDTH_WIDE,
        "`世` must take a WIDE (1) cell"
    );
    assert_eq!(
        cells[2].width(),
        WIDTH_SPACER_TAIL,
        "the cell after `世` must be its SPACER_TAIL (2): WIDE and SPACER_TAIL swapped or \
         collapsed would paint the glyph twice or not at all"
    );

    handle.shutdown();
}

/// `close_terminal` hangs the session up, and it leaves `live_terminals`.
#[test]
fn close_ends_the_session() {
    let Some(handle) = start() else {
        return;
    };
    let id = open(&handle, 0, TARGET_WINDOW, "exec sleep 600");
    assert!(
        handle.live_terminals().contains(&id),
        "the new session must be live before it is closed"
    );

    handle.close_terminal(id);
    let deadline = Instant::now() + TIMEOUT;
    while handle.live_terminals().contains(&id) && Instant::now() < deadline {
        sleep(POLL_INTERVAL);
    }
    assert!(
        !handle.live_terminals().contains(&id),
        "close_terminal must end the session within {TIMEOUT:?}; still live means it never \
         reached core"
    );

    handle.shutdown();
}

/// A split target opens a new window through an ordinary `Split` intent of the asked
/// orientation, and `bind_terminal` shows a live session in another window.
#[test]
fn split_target_and_bind() {
    let Some(handle) = start() else {
        return;
    };
    let original = active_window(&handle);
    let _ = handle.drain_intents();

    let id = open(&handle, original, TARGET_SPLIT_VERTICAL, "exec sleep 600");
    let deadline = Instant::now() + TIMEOUT;
    let mut split = None;
    let mut seen = Vec::new();
    while split.is_none() && Instant::now() < deadline {
        for intent in handle.drain_intents() {
            if intent.kind() == INTENT_SPLIT {
                split = Some((intent.orientation(), intent.win()));
            } else {
                seen.push(intent.kind());
            }
        }
        sleep(POLL_INTERVAL);
    }
    let (orientation, new_win) = split.unwrap_or_else(|| {
        panic!(
            "open_terminal with SPLIT_VERTICAL must split through a Split intent; none came, \
             only intents of kinds {seen:?}"
        )
    });
    assert_eq!(
        orientation, ORIENTATION_VERTICAL,
        "SPLIT_VERTICAL must split side by side (orientation 1); 0 means it maps to Horizontal"
    );
    poll("the split's new window to carry the session", || {
        handle.win_buffer(new_win).terminal() == id
    });
    assert_eq!(
        handle.win_buffer(original).terminal(),
        0,
        "the window the split came from still shows its own buffer, so its terminal() must be \
         0; the id means terminal() does not read the window asked about"
    );

    assert!(
        !handle.bind_terminal(id + 1000, original, TARGET_WINDOW),
        "bind_terminal of an id no session has must return false"
    );
    assert!(
        handle.bind_terminal(id, original, TARGET_WINDOW),
        "bind_terminal of a live session into a valid window must return true"
    );
    poll("the original window to show the bound session", || {
        handle.win_buffer(original).terminal() == id
    });

    handle.shutdown();
}

/// An unknown target opens nothing, and once the core is shut down every session call
/// answers the empty value without panicking.
#[test]
fn a_shut_down_handle_answers_empty() {
    let Some(handle) = start() else {
        return;
    };
    let win = active_window(&handle);
    assert_eq!(
        handle.open_terminal(0, TARGET_UNKNOWN, "exec sleep 600".to_owned()),
        0,
        "open_terminal with a target no terminal_target constant names must return 0"
    );
    assert!(
        handle.live_terminals().is_empty(),
        "an unknown target must start no session, but live_terminals() reads {:?}",
        handle.live_terminals()
    );
    let id = open(&handle, 0, TARGET_WINDOW, "printf 'GONE'; exec sleep 600");
    wait_frame(&handle, id, "the session's output", |frame| {
        row_text(frame, 0) == "GONE"
    });

    handle.shutdown();

    assert_eq!(
        handle.open_terminal(0, TARGET_WINDOW, "exec sleep 600".to_owned()),
        0,
        "open_terminal must return 0 once the core is gone"
    );
    let frame = handle.terminal_frame(id);
    assert_eq!(
        (frame.version(), frame.cols(), frame.rows()),
        (0, 0, 0),
        "a shut-down core's terminal_frame must be the empty frame, version 0 and 0x0"
    );
    assert!(
        frame.row_cells(0).is_empty() && !frame.has_cursor() && frame.title().is_empty(),
        "the empty frame must have no cells, no cursor and no title"
    );
    assert!(
        handle.live_terminals().is_empty(),
        "a shut-down core has no live session"
    );
    assert_eq!(
        handle.terminal_selection_text(id),
        "",
        "a shut-down core has no selection to copy"
    );
    assert_eq!(
        handle.win_buffer(win).terminal(),
        0,
        "a shut-down core binds no window to a session"
    );
    assert!(
        !handle.bind_terminal(id, win, TARGET_WINDOW),
        "bind_terminal must return false once the core is gone"
    );
    // None of these may panic on a gone core; each is a no-op.
    handle.terminal_key(id, named_key(KIND_ENTER));
    handle.terminal_text(id, "x".to_owned());
    handle.terminal_paste(id, "x".to_owned());
    handle.terminal_mouse(id, MOUSE_PRESS, BUTTON_LEFT, 1.0, 1.0, NO_MODS);
    handle.terminal_scroll(id, SCROLL_UP, 1, 1.0, 1.0, NO_MODS);
    handle.terminal_focus(id, true);
    handle.resize_terminal(id, 10, 10, 8, 16);
    handle.close_terminal(id);
}
