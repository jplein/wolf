//! Integration tests of the tab's last pane beside the terminal drawer: `:bd`, `:bd %`, `:bw`,
//! the quit family and `InputEvent::ClosePane`, which close the tab there, the close chords and
//! `:close`, which are refused there, and core's terminal router, each run on the tab's only
//! pane while the drawer is open, and `:bd` with the drawer closed. A differential case checks that the buffer `:bd` shows beside the drawer is the one
//! Neovim's own `:bd` picks from the jumplist with the drawer closed. A real, hermetic Neovim
//! holds the windows, and a real `/bin/sh` runs in the drawer's pty.
//!
//! The drawer is a Neovim window (`w:wolf_drawer`) that neither front-end's pane tree holds
//! (`specs/terminal-panes.md`, "The drawer"), so a close that counts Neovim's windows sees two
//! where the tab has one pane. Every drawer case opens the drawer with `toggle_terminal`, then
//! makes the editor window current with `focus_win`, as a front-end's focus move does. Ex
//! commands are typed with `send_ex`, so `init.lua`'s whole-word abbreviations fire as they do
//! for a user. The drawer runs the default shell, so every case pins `SHELL=/bin/sh` first
//! ([`common::pin_default_shell`]).
//!
//! ## Premises about the components
//!
//! Each is measured by these tests against the real component, not assumed:
//!
//! - Neovim's own `:bd` with the drawer closed keeps the tabpage's last window and shows
//!   another buffer there: the file edited before, or a new, unnamed, listed buffer when no
//!   other listed buffer exists (the drawer-closed cases and the startup-buffer case).
//! - The startup buffer is `Untitled.md`, named but not on disk, so Neovim does not reuse it in
//!   place when `:bd` needs an empty buffer, and the deleted buffer is left unlisted.
//! - Neovim's default `'jumpoptions'` holds `clean`, under which its own `:bd` removes the
//!   deleted buffer's entries from the window's jumplist, moves the index back by one for each
//!   removed entry before it, and shows the listed buffer it then finds at the index. C28
//!   compares the helper's pick with that one rather than naming a buffer, and checks the
//!   option and the jumplist it starts from in both cores.
//! - Two fresh cores given the same keys read the same `getjumplist()`, and opening the drawer
//!   then refocusing the editor window leaves that window's jumplist as it was (C28's premise).
//! - `probe_values` types its command behind every earlier key, so it reads Neovim's state
//!   after them. The drawer placeholder is `nofile` and not in terminal mode, so a probe typed
//!   while the drawer is current still reaches Neovim's command line.
//! - Neovim reports `E89` for `:bd` of a modified buffer without `!`, and `ext_messages` puts
//!   an error echoed into history in the snapshot's toast.
//! - `<C-w>c` and `<C-w>q` sent to a session go through core's router to `wolf_terminal_close`
//!   for that session's window, whichever window is current.

mod common;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::{
    ch, ctrl, key, poll_until, probe_values, send_ex, settle_for, start_core_at, term_ctrl,
    term_key, type_str, FlushWaiter, FLUSH_TIMEOUT, POLL_INTERVAL,
};
use wolf_core::{Command, InputEvent, Key, TerminalId, WolfCore};

/// How long a close that must not happen is given to happen anyway.
const NEGATIVE_SETTLE: Duration = Duration::from_millis(1500);

/// How long the snapshot is given to follow Neovim's current window after a probe read it.
const ACTIVE_SETTLE: Duration = Duration::from_millis(500);

/// The file edited first in the editor window: the one `:bd` of [`SECOND`] must show again.
const FIRST: &str = "alpha.md";

/// The file edited second, current when `:bd` or `:bw` runs.
const SECOND: &str = "beta.md";

/// The line typed into a buffer to make it modified.
const MARKER: &str = "typed beside the drawer";

/// The lines C28 jumps to inside [`SECOND`], in order, each leaving a jumplist entry behind.
const JUMP_LINES: [usize; 3] = [3, 6, 9];

/// How many lines C28 writes into [`SECOND`]: enough for every one of [`JUMP_LINES`].
const SECOND_LINES: usize = 12;

/// The files C28 edits after its jumps inside [`SECOND`], in order.
const LATER: [&str; 2] = ["gamma.md", "delta.md"];

/// A core running `/bin/sh` as its default shell, in a fresh canonical cwd holding [`FIRST`]
/// and [`SECOND`].
struct Fixture {
    core: WolfCore,
    waiter: FlushWaiter,
    cwd: PathBuf,
}

impl Fixture {
    fn start() -> Fixture {
        common::pin_default_shell();
        let cwd =
            std::fs::canonicalize(common::fresh_cwd()).expect("failed to canonicalize the cwd");
        for name in [FIRST, SECOND] {
            std::fs::write(cwd.join(name), format!("# {name}\n"))
                .unwrap_or_else(|e| panic!("failed to write {name}: {e}"));
        }
        let (core, waiter) = start_core_at(cwd.clone());
        Fixture { core, waiter, cwd }
    }

    /// Evaluates `exprs` in Neovim, behind every input sent before.
    fn probe(&self, exprs: &[String]) -> Vec<String> {
        let exprs: Vec<&str> = exprs.iter().map(String::as_str).collect();
        probe_values(&self.core, &exprs, &self.cwd.join("probe.txt"))
    }

    /// One value of [`Fixture::probe`].
    fn probe1(&self, expr: &str) -> String {
        self.probe(&[expr.to_owned()])
            .pop()
            .expect("probe_values returns one value per expression")
    }

    /// Edits `name` in the current window and returns its buffer number.
    fn edit(&self, name: &str) -> String {
        send_ex(&self.core, &format!("e {name}"));
        let values = self.probe(&["bufnr()".to_owned(), "expand('%:t')".to_owned()]);
        assert_eq!(
            values[1], name,
            "premise: `:e {name}` must make {name} current; the current buffer is {:?}",
            values[1]
        );
        values[0].clone()
    }

    /// Edits [`FIRST`], then [`SECOND`], in the startup window, and returns the window and
    /// both buffer numbers.
    fn edit_both(&self) -> (i64, String, String) {
        let first = self.edit(FIRST);
        let second = self.edit(SECOND);
        (self.current_win(), first, second)
    }

    /// Neovim's current window.
    fn current_win(&self) -> i64 {
        let win = self.probe1("win_getid()");
        win.parse()
            .unwrap_or_else(|e| panic!("win_getid() read {win:?}, not a window id: {e}"))
    }

    /// Toggles the drawer open, waits for its window to carry a session, then makes `editor`
    /// current again, as a front-end's focus move does. Returns the drawer's window and
    /// session.
    fn open_drawer_beside(&self, editor: i64) -> (i64, TerminalId) {
        let (drawer, id) = self.open_drawer();
        self.core.focus_win(editor);
        poll_until(
            &self.waiter,
            "focus_win to make the editor window current beside the drawer (premise)",
            || self.core.read_snapshot().active_win,
            |active| *active == Some(editor),
        );
        (drawer, id)
    }

    /// Toggles the drawer open and waits for its window to carry a session, leaving it current.
    fn open_drawer(&self) -> (i64, TerminalId) {
        self.core.toggle_terminal();
        let (win, id) = poll_until(
            &self.waiter,
            "toggle_terminal to open a drawer window carrying a session (premise)",
            || drawer_window(&self.core),
            |drawer| matches!(drawer, Some((_, Some(_)))),
        )
        .expect("poll_until returned only once a drawer window existed");
        (
            win,
            id.expect("poll_until returned only once the drawer window carried a session"),
        )
    }

    /// Types a new first line into the current buffer, leaving it modified.
    fn modify(&self) {
        type_str(&self.core, "ggO");
        type_str(&self.core, MARKER);
        self.core.send_input(key(Key::Esc));
        assert_eq!(
            self.probe1("&modified"),
            "1",
            "premise: the typed line must leave the current buffer modified"
        );
    }

    /// Asserts `active_win` names `win` once the snapshot has had [`ACTIVE_SETTLE`] to follow
    /// Neovim, where `what` says what made the change.
    fn assert_active(&self, win: i64, what: &str) {
        settle_for(&self.waiter, ACTIVE_SETTLE);
        let active = self.core.read_snapshot().active_win;
        assert_eq!(
            active,
            Some(win),
            "after {what}, active_win must name the window {win} Neovim reports current; \
             {active:?} means the front-ends would show another pane, or none, as active."
        );
    }

    fn shutdown(self) {
        self.core.shutdown().expect("shutdown should be Ok");
    }
}

/// The window flagged `is_drawer` in `win_buffers`, with the session it shows.
fn drawer_window(core: &WolfCore) -> Option<(i64, Option<TerminalId>)> {
    core.read_snapshot()
        .win_buffers
        .iter()
        .find(|(_, buffer)| buffer.is_drawer)
        .map(|(win, buffer)| (*win, buffer.terminal))
}

/// Sends `<C-w>` then `key` to Neovim in normal mode, as an editor pane's keys go.
fn nvim_chord(core: &WolfCore, next: char) {
    core.send_input(ctrl(Key::Char('w')));
    core.send_input(ch(next));
}

/// Sends `<C-w>` then `next` to session `id`, through core's router.
fn session_chord(core: &WolfCore, id: TerminalId, next: char) {
    core.terminal_key(id, term_ctrl(Key::Char('w')));
    core.terminal_key(id, term_key(Key::Char(next)));
}

/// Drains commands for up to `within`, returning whether `Command::CloseTab` was one.
fn close_tab_drains(core: &WolfCore, waiter: &FlushWaiter, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    loop {
        if core.drain_commands().contains(&Command::CloseTab) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// The toast's lines, each its chunks joined, joined with newlines.
fn toast_text(core: &WolfCore) -> String {
    core.read_snapshot()
        .toast
        .lines()
        .iter()
        .map(|line| line.iter().map(|chunk| chunk.text.as_str()).collect())
        .collect::<Vec<String>>()
        .join("\n")
}

/// `:bd` or `:bw` of [`SECOND`] in the tab's only editor window beside the drawer keeps the
/// window, current, on [`FIRST`]; the drawer keeps its session. Returns the fixture and
/// [`SECOND`]'s buffer number.
fn delete_beside_the_drawer(cmd: &str) -> (Fixture, String) {
    let f = Fixture::start();
    let (editor, first, second) = f.edit_both();
    let (drawer, id) = f.open_drawer_beside(editor);

    send_ex(&f.core, cmd);
    let values = f.probe(&[
        format!("win_id2win({editor}) > 0"),
        format!("winbufnr({editor})"),
        "win_getid()".to_owned(),
    ]);
    assert_eq!(
        values,
        ["1".to_owned(), first.clone(), editor.to_string()],
        "`:{cmd}` of {SECOND} (buffer {second}) in the tab's only editor window {editor}, \
         beside the drawer {drawer}, must keep that window, show {FIRST} (buffer {first}) in it \
         and leave it current: (window open, its buffer, current window). (0, -1, {drawer}) \
         means Neovim closed the editor window because the drawer kept it from being the \
         tabpage's last, leaving the pane tree empty and the drawer current; another buffer \
         means the replacement is not the one Neovim shows with the drawer closed."
    );
    f.assert_active(editor, &format!("`:{cmd}` beside the drawer"));
    assert_eq!(
        drawer_window(&f.core),
        Some((drawer, Some(id))),
        "the drawer window {drawer} must still show its session {id:?} after `:{cmd}` in the \
         editor window."
    );
    (f, second)
}

/// C1: `:bd` of the current buffer in the tab's only editor window beside the drawer keeps the
/// window, current, on the file shown before; the deleted buffer stays, unlisted.
#[test]
fn bd_beside_the_drawer_keeps_the_last_pane_on_the_previous_file() {
    let (f, second) = delete_beside_the_drawer("bd");
    let values = f.probe(&[
        format!("buflisted({second})"),
        format!("bufexists({second})"),
    ]);
    assert_eq!(
        values,
        ["0", "1"],
        "`:bd` must delete {SECOND} (buffer {second}) as Neovim's :bdelete does: (listed, \
         exists) must read (0, 1). (1, 1) means the native command never ran after the switch; \
         (0, 0) means it ran :bwipeout."
    );
    f.shutdown();
}

/// C2: `:bd` of the startup buffer, the only listed one, in the tab's only editor window beside
/// the drawer keeps the window, current, on a new unnamed listed buffer that is not the
/// drawer's placeholder.
#[test]
fn bd_of_the_only_listed_buffer_beside_the_drawer_shows_a_new_empty_buffer() {
    let f = Fixture::start();
    let values = f.probe(&[
        "bufnr()".to_owned(),
        "win_getid()".to_owned(),
        "len(getbufinfo({'buflisted': 1}))".to_owned(),
    ]);
    let (startup, editor) = (values[0].clone(), values[1].clone());
    assert_eq!(
        values[2], "1",
        "premise: the startup buffer {startup} must be the only listed buffer"
    );
    let editor: i64 = editor.parse().expect("win_getid() is a window id");
    let (drawer, _id) = f.open_drawer_beside(editor);

    send_ex(&f.core, "bd");
    let shown = format!("winbufnr({editor})");
    let values = f.probe(&[
        format!("win_id2win({editor}) > 0"),
        "win_getid()".to_owned(),
        format!("{shown} != {startup}"),
        format!("bufname({shown})"),
        format!("buflisted({shown})"),
        format!("getbufvar({shown}, 'wolf_terminal_id', 'none')"),
        format!("buflisted({startup})"),
    ]);
    assert_eq!(
        values,
        [
            "1".to_owned(),
            editor.to_string(),
            "1".to_owned(),
            String::new(),
            "1".to_owned(),
            "none".to_owned(),
            "0".to_owned(),
        ],
        "`:bd` of the only listed buffer {startup} in the editor window {editor} beside the \
         drawer {drawer} must read (window open, current window, another buffer shown, its \
         name, its listed flag, its b:wolf_terminal_id, the old buffer listed) as (1, {editor}, \
         1, empty, 1, none, 0). A closed window with the drawer current means Neovim closed the \
         last pane; a session id means the drawer's unlisted placeholder was picked as the \
         replacement; an unlisted replacement means the new buffer was made unlisted."
    );
    f.assert_active(editor, "`:bd` of the only listed buffer beside the drawer");
    f.shutdown();
}

/// C3: `:bw` behaves as `:bd` beside the drawer, and wipes the buffer.
#[test]
fn bw_beside_the_drawer_keeps_the_last_pane_on_the_previous_file() {
    let (f, second) = delete_beside_the_drawer("bw");
    assert_eq!(
        f.probe1(&format!("bufexists({second})")),
        "0",
        "`:bw` must wipe {SECOND} (buffer {second}); 1 means it ran :bdelete, or no native \
         command at all."
    );
    f.shutdown();
}

/// C27: `:bd %` beside the drawer deletes the buffer `%` named when it was typed, not the
/// replacement the window shows by the time Neovim's command runs.
#[test]
fn bd_percent_beside_the_drawer_deletes_the_shown_buffer() {
    let (f, second) = delete_beside_the_drawer("bd %");
    let values = f.probe(&[
        format!("buflisted({second})"),
        format!("bufexists({second})"),
        "buflisted(bufnr())".to_owned(),
    ]);
    assert_eq!(
        values,
        ["0", "1", "1"],
        "`:bd %` must delete {SECOND} (buffer {second}) and leave {FIRST}, now shown, alone: \
         ({SECOND} listed, {SECOND} exists, {FIRST} listed) must read (0, 1, 1). (1, 1, 0) means \
         `%` was resolved after the switch, so Neovim deleted the replacement {FIRST} instead; \
         (0, 0, 1) means it ran :bwipeout."
    );
    f.shutdown();
}

/// What C28 reads in one core: the editor window's jumplist right before `:bd`, as `name:lnum`
/// entries, its index, and the file `:bd` then shows in that window.
#[derive(Debug)]
struct JumpPick {
    jumps: Vec<String>,
    index: usize,
    shown: String,
}

/// Builds a jumplist with three entries of [`SECOND`] before its index and two other files
/// after, back inside [`SECOND`], then runs `:bd` there, beside the drawer when `beside`.
fn bd_after_jumps(beside: bool) -> JumpPick {
    let place = if beside {
        "beside the drawer"
    } else {
        "with the drawer closed"
    };
    let f = Fixture::start();
    let lines: String = (1..=SECOND_LINES).map(|n| format!("line {n}\n")).collect();
    std::fs::write(f.cwd.join(SECOND), lines).expect("failed to rewrite the second file");
    for name in LATER {
        std::fs::write(f.cwd.join(name), format!("# {name}\n"))
            .unwrap_or_else(|e| panic!("failed to write {name}: {e}"));
    }
    f.edit(FIRST);
    f.edit(SECOND);
    let editor = f.current_win();
    for line in JUMP_LINES {
        type_str(&f.core, &format!("{line}G"));
    }
    assert_eq!(
        f.probe1("line('.')"),
        JUMP_LINES[JUMP_LINES.len() - 1].to_string(),
        "premise: the typed `<n>G` jumps must move the cursor inside {SECOND}"
    );
    for name in LATER {
        f.edit(name);
    }
    let mut current = String::new();
    for _ in 0..=LATER.len() {
        f.core.send_input(ctrl(Key::Char('o')));
        current = f.probe1("expand('%:t')");
        if current == SECOND {
            break;
        }
    }
    assert_eq!(
        current, SECOND,
        "premise: <C-o> must walk the jumplist back into {SECOND} {place}"
    );
    if beside {
        f.open_drawer_beside(editor);
    }

    let values = f.probe(&[
        "&jumpoptions".to_owned(),
        format!(
            "join(map(getjumplist({editor})[0], \
             {{_, j -> fnamemodify(bufname(j.bufnr), ':t') .. ':' .. j.lnum}}))"
        ),
        format!("getjumplist({editor})[1]"),
    ]);
    assert!(
        format!(",{},", values[0]).contains(",clean,"),
        "premise: 'jumpoptions' must hold clean {place}, the only setting under which `:bd` \
         prunes the jumplist; it reads {:?}",
        values[0]
    );
    let jumps: Vec<String> = values[1].split_whitespace().map(String::from).collect();
    let index: usize = values[2]
        .parse()
        .unwrap_or_else(|e| panic!("getjumplist()[1] read {:?}: {e}", values[2]));
    let in_second = |entry: &String| entry.starts_with(&format!("{SECOND}:"));
    assert!(
        jumps.get(index).is_some_and(in_second),
        "premise: the jumplist index {index} must be inside the list, on a {SECOND} entry, \
         {place}; the list reads {jumps:?}"
    );
    let before = jumps[..index]
        .iter()
        .filter(|entry| in_second(entry))
        .count();
    assert!(
        before >= 2,
        "premise: at least two {SECOND} entries must come before the index {index} {place}, \
         so the index moves back more than once; the list reads {jumps:?}"
    );

    send_ex(&f.core, "bd");
    let values = f.probe(&[
        format!("win_id2win({editor}) > 0"),
        "win_getid()".to_owned(),
        format!(
            "win_id2win({editor}) > 0 ? fnamemodify(bufname(winbufnr({editor})), ':t') : 'gone'"
        ),
    ]);
    assert_eq!(
        values[..2],
        ["1".to_owned(), editor.to_string()],
        "`:bd` of {SECOND} in the tab's only editor window {editor} {place} must keep that \
         window open and current: (window open, current window). A closed window means the \
         last pane was closed; with the drawer closed that is a broken premise about Neovim."
    );
    if beside {
        f.assert_active(editor, "`:bd` after jumps beside the drawer");
    }
    let shown = values[2].clone();
    f.shutdown();
    JumpPick {
        jumps,
        index,
        shown,
    }
}

/// C28: from the same jumplist, `:bd` beside the drawer shows the buffer Neovim's own `:bd`
/// shows with the drawer closed, with two entries of the deleted buffer before the index.
#[test]
fn bd_beside_the_drawer_shows_neovims_jumplist_pick() {
    let neovim = bd_after_jumps(false);
    let beside = bd_after_jumps(true);
    assert_eq!(
        (&beside.jumps, beside.index),
        (&neovim.jumps, neovim.index),
        "premise: both cores must reach `:bd` with the same jumplist and index in the editor \
         window; opening the drawer must not change them"
    );
    assert_eq!(
        beside.shown, neovim.shown,
        "`:bd` of {SECOND} beside the drawer must show {:?}, the file Neovim's own `:bd` shows \
         with the drawer closed from the jumplist {:?} at index {}. Another file means the \
         helper's jumplist walk starts elsewhere, as when it moves the index back for fewer of \
         the removed entries before it than Neovim does.",
        neovim.shown, neovim.jumps, neovim.index
    );
}

/// C4: with two editor windows on the same buffer beside the drawer, `:bd` closes one and
/// keeps the other, current, on the file shown before, as Neovim does with the drawer closed.
#[test]
fn bd_on_two_windows_of_one_buffer_beside_the_drawer_keeps_one() {
    let f = Fixture::start();
    let (left, first, second) = f.edit_both();
    nvim_chord(&f.core, 'v');
    let right = f.current_win();
    assert_ne!(
        right, left,
        "premise: <C-w>v must split the editor window {left} and make the new window current"
    );
    let (drawer, _id) = f.open_drawer_beside(right);

    send_ex(&f.core, "bd");
    let not_drawer = "{_, w -> !getwinvar(w.winid, 'wolf_drawer', 0)}";
    let values = f.probe(&[
        format!("len(filter(getwininfo(), {not_drawer}))"),
        format!("win_id2win({left}) > 0 || win_id2win({right}) > 0"),
        "getwinvar(win_getid(), 'wolf_drawer', 0)".to_owned(),
        "bufnr()".to_owned(),
        format!("win_id2win({drawer}) > 0"),
        format!("buflisted({second})"),
    ]);
    assert_eq!(
        values,
        [
            "1".to_owned(),
            "1".to_owned(),
            "0".to_owned(),
            first.clone(),
            "1".to_owned(),
            "0".to_owned(),
        ],
        "`:bd` of {SECOND} (buffer {second}) shown in the editor windows {left} and {right} \
         beside the drawer {drawer} must read (editor windows left, one of the two open, current \
         is the drawer, current buffer, drawer open, {SECOND} listed) as (1, 1, 0, {first}, 1, \
         0). No editor window with the drawer current means both closed, as Neovim does when \
         the drawer keeps either from being the last; two mean the second window was kept."
    );
    let current = f.current_win();
    f.assert_active(current, "`:bd` on two windows beside the drawer");
    f.shutdown();
}

/// C5: with the drawer never opened, `:bd` keeps the tab's only window, current, on the file
/// shown before, and leaves the deleted buffer unlisted.
#[test]
fn bd_without_the_drawer_keeps_the_window_on_the_previous_file() {
    let f = Fixture::start();
    let (editor, first, second) = f.edit_both();

    send_ex(&f.core, "bd");
    let values = f.probe(&[
        "win_getid()".to_owned(),
        format!("winbufnr({editor})"),
        format!("buflisted({second})"),
        format!("bufexists({second})"),
        "winnr('$')".to_owned(),
    ]);
    assert_eq!(
        values,
        [
            editor.to_string(),
            first.clone(),
            "0".to_owned(),
            "1".to_owned(),
            "1".to_owned(),
        ],
        "`:bd` with the drawer closed must read (current window, its buffer, {SECOND} listed, \
         {SECOND} exists, window count) as ({editor}, {first}, 0, 1, 1), as Neovim's own \
         :bdelete does. A wiped buffer means `bd` reaches :bwipeout; another shown buffer means \
         the drawer-closed path no longer runs Neovim's command alone."
    );
    f.shutdown();
}

/// C6: with the drawer never opened, `:bd <n>` of a hidden buffer unlists that buffer and
/// leaves the current window on its own buffer.
#[test]
fn bd_of_a_hidden_buffer_by_number_deletes_that_buffer() {
    let f = Fixture::start();
    let (editor, first, second) = f.edit_both();

    send_ex(&f.core, &format!("bd {first}"));
    let values = f.probe(&[
        format!("buflisted({first})"),
        "win_getid()".to_owned(),
        format!("winbufnr({editor})"),
    ]);
    assert_eq!(
        values,
        ["0".to_owned(), editor.to_string(), second.clone()],
        "`:bd {first}` must delete the hidden {FIRST} and leave the window {editor} on {SECOND} \
         (buffer {second}): (listed, current window, its buffer). {FIRST} still listed with \
         another buffer shown means the argument was dropped and the current buffer deleted."
    );
    f.shutdown();
}

/// C7: `:bd` of a modified buffer beside the drawer is refused with `E89` and changes nothing:
/// the editor window still shows the buffer, listed and modified.
#[test]
fn bd_of_a_modified_buffer_beside_the_drawer_is_refused_with_e89() {
    let f = Fixture::start();
    let (editor, first, second) = f.edit_both();
    f.modify();
    let (drawer, _id) = f.open_drawer_beside(editor);

    send_ex(&f.core, "bd");
    poll_until(
        &f.waiter,
        "the toast to show E89 after `:bd` of a modified buffer beside the drawer; none means \
         the refusal was swallowed without an echo",
        || toast_text(&f.core),
        |text| text.contains("E89"),
    );
    let values = f.probe(&[
        format!("win_id2win({editor}) > 0"),
        "win_getid()".to_owned(),
        format!("winbufnr({editor})"),
        format!("buflisted({second})"),
        format!("getbufvar({second}, '&modified')"),
    ]);
    assert_eq!(
        values,
        [
            "1".to_owned(),
            editor.to_string(),
            second.clone(),
            "1".to_owned(),
            "1".to_owned(),
        ],
        "a refused `:bd` of the modified {SECOND} (buffer {second}) beside the drawer {drawer} \
         must leave (window open, current window, its buffer, listed, modified) at (1, {editor}, \
         {second}, 1, 1). Buffer {first} shown means the window was switched before the refusal, \
         so a modified target was treated as deletable."
    );
    f.shutdown();
}

/// What a quit-family case types, and whether it edits the buffer first.
struct Quit {
    /// How the case reads in messages.
    name: &'static str,
    /// Whether the buffer is modified before the quit.
    modify: bool,
    /// Whether the quit must write the typed line to disk.
    writes: bool,
    /// Sends the quit to Neovim.
    send: fn(&WolfCore),
}

/// The quit on the tab's only editor window beside the drawer asks the front-end to close
/// the tab, as on a last pane with the drawer closed, and leaves the editor window open.
fn quit_beside_the_drawer(quit: Quit) {
    let f = Fixture::start();
    let first = f.edit(FIRST);
    let editor = f.current_win();
    if quit.modify {
        f.modify();
    }
    let (drawer, _id) = f.open_drawer_beside(editor);
    let _ = f.core.drain_commands();

    (quit.send)(&f.core);
    let drained = close_tab_drains(&f.core, &f.waiter, FLUSH_TIMEOUT);
    let open = f.probe(&[
        format!("win_id2win({editor}) > 0"),
        format!("win_id2win({drawer}) > 0"),
    ]);
    assert_eq!(
        (drained, open[0].as_str()),
        (true, "1"),
        "{} on the tab's only editor window {editor} (buffer {first}) beside the drawer \
         {drawer} must drain Command::CloseTab and leave the window open: (CloseTab, \
         window open). (false, 0) means the quit closed the editor as a split, because the \
         drawer was counted as a pane; (false, 1) means the quit was refused or never ran.",
        quit.name
    );
    if quit.writes {
        let path = f.cwd.join(FIRST);
        let text = std::fs::read_to_string(&path).expect("failed to read the file back");
        assert!(
            text.starts_with(MARKER),
            "{} must write the typed line to {FIRST} before the close; it holds {text:?}",
            quit.name
        );
    }
    f.shutdown();
}

/// C8: `:q` on the last pane beside the drawer.
#[test]
fn q_beside_the_drawer_closes_the_tab() {
    quit_beside_the_drawer(Quit {
        name: "`:q`",
        modify: false,
        writes: false,
        send: |core| send_ex(core, "q"),
    });
}

/// C9: `:wq` on the modified last pane beside the drawer writes it, then closes the tab.
#[test]
fn wq_beside_the_drawer_writes_and_closes_the_tab() {
    quit_beside_the_drawer(Quit {
        name: "`:wq`",
        modify: true,
        writes: true,
        send: |core| send_ex(core, "wq"),
    });
}

/// C10: `:x` on the modified last pane beside the drawer writes it, then closes the tab.
#[test]
fn x_beside_the_drawer_writes_and_closes_the_tab() {
    quit_beside_the_drawer(Quit {
        name: "`:x`",
        modify: true,
        writes: true,
        send: |core| send_ex(core, "x"),
    });
}

/// C11: `ZZ` on the modified last pane beside the drawer writes it, then closes the tab.
#[test]
fn zz_beside_the_drawer_writes_and_closes_the_tab() {
    quit_beside_the_drawer(Quit {
        name: "`ZZ`",
        modify: true,
        writes: true,
        send: |core| type_str(core, "ZZ"),
    });
}

/// C12: `ZQ` on the modified last pane beside the drawer closes the tab.
#[test]
fn zq_beside_the_drawer_closes_the_tab() {
    quit_beside_the_drawer(Quit {
        name: "`ZQ`",
        modify: true,
        writes: false,
        send: |core| type_str(core, "ZQ"),
    });
}

/// A close on the tab's only editor window beside the drawer is refused: both windows stay,
/// and the front-end is not asked to close the tab.
fn close_refused_beside_the_drawer(name: &str, send: impl Fn(&WolfCore)) {
    let f = Fixture::start();
    let first = f.edit(FIRST);
    let editor = f.current_win();
    let (drawer, id) = f.open_drawer_beside(editor);
    let _ = f.core.drain_commands();

    send(&f.core);
    let drained = close_tab_drains(&f.core, &f.waiter, NEGATIVE_SETTLE);
    let values = f.probe(&[
        format!("win_id2win({editor}) > 0"),
        format!("win_id2win({drawer}) > 0"),
        format!("winbufnr({editor})"),
    ]);
    assert_eq!(
        (values, drained),
        (vec!["1".to_owned(), "1".to_owned(), first.clone()], false),
        "{name} on the tab's only editor window {editor} beside the drawer {drawer} must be \
         refused: ((editor open, drawer open, editor's buffer), CloseTab) must read ((1, 1, \
         {first}), false). An editor window gone means the close counted the drawer as a pane \
         and closed the last one; CloseTab means it was treated as a quit."
    );
    assert_eq!(
        f.core.live_terminals(),
        vec![id],
        "a refused {name} must leave the drawer's session {id:?} running."
    );
    f.shutdown();
}

/// C13: a typed `:close` on the last pane beside the drawer is refused.
#[test]
fn close_beside_the_drawer_is_refused() {
    close_refused_beside_the_drawer("`:close`", |core| send_ex(core, "close"));
}

/// C14: normal-mode `<C-w>c` on the last pane beside the drawer is refused.
#[test]
fn ctrl_w_c_beside_the_drawer_is_refused() {
    close_refused_beside_the_drawer("normal-mode `<C-w>c`", |core| nvim_chord(core, 'c'));
}

/// C15: normal-mode `<C-w>q` on the last pane beside the drawer is refused.
#[test]
fn ctrl_w_q_beside_the_drawer_is_refused() {
    close_refused_beside_the_drawer("normal-mode `<C-w>q`", |core| nvim_chord(core, 'q'));
}

/// C18: `InputEvent::ClosePane` on the last pane beside the drawer closes the tab, as `:q` does.
#[test]
fn close_pane_on_the_last_pane_beside_the_drawer_closes_the_tab() {
    quit_beside_the_drawer(Quit {
        name: "InputEvent::ClosePane",
        modify: false,
        writes: false,
        send: |core| core.send_input(InputEvent::ClosePane),
    });
}

/// Waits for window `win` to close, then asserts the editor window stays and the drawer's
/// session keeps running, where `what` names the close.
fn assert_drawer_closed(f: &Fixture, drawer: i64, id: TerminalId, editor: i64, what: &str) {
    poll_until(
        &f.waiter,
        &format!(
            "{what} to close the drawer window {drawer}; it staying means the drawer is refused \
             as the tab's last pane"
        ),
        || f.core.read_snapshot().win_buffers.contains_key(&drawer),
        |present| !*present,
    );
    assert_eq!(
        f.probe1(&format!("win_id2win({editor}) > 0")),
        "1",
        "{what} must close only the drawer; the editor window {editor} is gone."
    );
    settle_for(&f.waiter, NEGATIVE_SETTLE);
    assert_eq!(
        (f.core.live_terminals(), f.core.drawer_terminal()),
        (vec![id], Some(id)),
        "{what} must hide the drawer and keep its session {id:?} live and named by \
         drawer_terminal()."
    );
}

/// C16: `<C-w>q` sent to the drawer's session closes the drawer, keeps the editor window, and
/// keeps the session.
#[test]
fn ctrl_w_q_in_the_drawer_closes_only_the_drawer() {
    let f = Fixture::start();
    let editor = f.current_win();
    let (drawer, id) = f.open_drawer();

    session_chord(&f.core, id, 'q');
    assert_drawer_closed(&f, drawer, id, editor, "<C-w>q in the drawer's session");
    f.shutdown();
}

/// C19: `InputEvent::ClosePane` with the drawer current closes the drawer, keeps the editor
/// window, and keeps the session.
#[test]
fn close_pane_on_the_drawer_closes_only_the_drawer() {
    let f = Fixture::start();
    let editor = f.current_win();
    let (drawer, id) = f.open_drawer();
    poll_until(
        &f.waiter,
        "the opened drawer to be Neovim's current window (premise)",
        || f.core.read_snapshot().active_win,
        |active| *active == Some(drawer),
    );

    f.core.send_input(InputEvent::ClosePane);
    assert_drawer_closed(&f, drawer, id, editor, "ClosePane on the drawer");
    f.shutdown();
}

/// C17: `<C-w>c` sent to a terminal pane that is the tab's only pane beside the drawer is
/// refused: the pane's window keeps its session, and the session keeps running.
#[test]
fn ctrl_w_c_on_a_lone_terminal_pane_beside_the_drawer_is_refused() {
    let f = Fixture::start();
    let pane = f.current_win();
    send_ex(&f.core, "terminal");
    let session = poll_until(
        &f.waiter,
        "a typed `:terminal` to bind a session in the only window (premise)",
        || common::win_terminal(&f.core, pane),
        Option::is_some,
    )
    .expect("poll_until returned only once the window carried a session");
    let (drawer, drawer_id) = f.open_drawer();
    assert_ne!(
        drawer_id, session,
        "premise: the drawer must run a session of its own, not the pane's {session:?}"
    );

    session_chord(&f.core, session, 'c');
    settle_for(&f.waiter, NEGATIVE_SETTLE);
    let mut live = f.core.live_terminals();
    live.sort();
    let mut want = vec![session, drawer_id];
    want.sort();
    assert_eq!(
        (common::win_terminal(&f.core, pane), live),
        (Some(session), want),
        "<C-w>c in the terminal pane {pane}, the tab's only pane beside the drawer {drawer}, \
         must be refused: (the pane's session, live sessions). No session in the window, and \
         {session:?} gone from the live ones, means wolf_terminal_close counted the drawer and \
         closed the last pane, wiping its placeholder."
    );
    f.shutdown();
}

/// C29: `InputEvent::ClosePane` on a terminal pane that is the tab's only pane beside the
/// drawer asks the front-end to close the tab, and leaves both sessions to the tab's shutdown.
#[test]
fn close_pane_on_a_lone_terminal_pane_beside_the_drawer_closes_the_tab() {
    let f = Fixture::start();
    let pane = f.current_win();
    send_ex(&f.core, "terminal");
    let session = poll_until(
        &f.waiter,
        "a typed `:terminal` to bind a session in the only window (premise)",
        || common::win_terminal(&f.core, pane),
        Option::is_some,
    )
    .expect("poll_until returned only once the window carried a session");
    let (drawer, drawer_id) = f.open_drawer_beside(pane);
    assert_ne!(
        drawer_id, session,
        "premise: the drawer must run a session of its own, not the pane's {session:?}"
    );
    let _ = f.core.drain_commands();

    f.core.send_input(InputEvent::ClosePane);
    let drained = close_tab_drains(&f.core, &f.waiter, FLUSH_TIMEOUT);
    let mut live = f.core.live_terminals();
    live.sort();
    let mut want = vec![session, drawer_id];
    want.sort();
    assert_eq!(
        (drained, common::win_terminal(&f.core, pane), live),
        (true, Some(session), want),
        "ClosePane in the terminal pane {pane}, the tab's only pane beside the drawer {drawer}, \
         must drain Command::CloseTab and leave the pane and both sessions to the front-end's \
         close of the tab: (CloseTab, the pane's session, live sessions). (false, None, without \
         {session:?}) means the drawer was counted as a pane and the terminal pane closed as a \
         split; (false, Some, both) means the close was refused, as wolf_terminal_close refuses \
         the last pane."
    );
    f.shutdown();
}
