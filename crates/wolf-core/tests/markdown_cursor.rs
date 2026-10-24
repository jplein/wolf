//! Integration tests for `wolf-core`'s Markdown-preview CURSOR signal: the
//! snapshot exposes, per SOURCE Markdown buffer, the editing cursor's position via
//! `Snapshot::markdown_cursors: HashMap<i64, MarkdownCursor>`.
//!
//! The value is delivered by the `CursorMoved`/`CursorMovedI` autocmds installed by
//! `_G.wolf_setup_preview(chan)` in `runtime/nvim/init.lua`, which `rpcnotify` the
//! `wolf_markdown_cursor` event with THREE FLAT SCALARS —
//! `(source_bufnr, line('.'), charcol('.') - 1)`. Core UPSERTS them into
//! `markdown_cursors`, keyed by the SOURCE buffer number (exactly like
//! `markdown_ticks`), which is what lets the FFI `previews()` join hand each preview
//! window its source buffer's cursor.
//!
//! ## The contracts pinned here
//!
//!  * `line` is 1-BASED (it is `line('.')`, matching the `wl-{L}-{C}` marker
//!    scheme's `L` emitted by `crates/wolf-core/src/markdown.rs`).
//!  * `col` is a 0-BASED **CHARACTER** column (`charcol('.') - 1`), NOT a byte
//!    column — the marker scheme's `C`. A regression to `col('.')` is caught
//!    loudly by the multibyte test.
//!  * The cursor signal is INDEPENDENT of the dirty signal: a pure normal-mode
//!    motion updates `markdown_cursors` and leaves `markdown_ticks` alone.
//!  * A markdown buffer that is merely OPENED — and never touched — already
//!    reports a cursor. `:edit` fires `CursorMoved` BEFORE filetype detection, so
//!    the steady-state autocmds alone would leave an untouched buffer with no entry
//!    at all; `BufEnter`/`BufWinEnter` are what close that gap.
//!  * Non-markdown buffers and PREVIEW SCRATCH buffers never become keys.
//!  * Neither does a markdown buffer loaded in the BACKGROUND. `vim.fn.bufload()`
//!    fires `BufEnter`/`BufWinEnter` inside nvim's synthetic AUTOCMD WINDOW with the
//!    target buffer genuinely current, which defeats the current-buffer, filetype
//!    and preview-scratch guards all at once; only `win_gettype() ~= ""` rejects it.
//!  * `markdown_cursors` and `markdown_ticks` are keyed by the SAME source bufnr,
//!    which is also `PreviewInfo::source_bufnr` — the join the FFI depends on.
//!
//! Because the notification is out-of-band (it wakes but does NOT bump the redraw
//! `seq`), every assertion POLLS `read_snapshot()` with a generous deadline,
//! draining wakes as it goes, and panics LOUDLY with the last observed maps.
//! `seq` invariance is deliberately NOT tested: a cursor motion legitimately
//! produces its own redraw batch (`cursor_goto` + `flush`), so it is enforced
//! structurally (the handler never touches `snap.seq`) and by code review.
//!
//! All tests drive a REAL, hermetic nvim (no mocks) via `WOLF_NVIM_BIN`; the common
//! helpers panic loudly if none can be resolved (wolf-core has no SKIP path).

mod common;

use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, Instant};

use common::{ch, key, type_str, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{Key, MarkdownCursor, Snapshot, WolfCore};

/// A plain multi-line Markdown document. Line 1 is a heading; lines 2-4 are short
/// single-word paragraphs, so `j`/`l` motions land on predictable columns.
const DOC: &str = "# Cursor\nalpha\nbravo\ncharlie\n";

/// The content of `DOC`'s line 2, used by the insert-mode past-EOL test. Its
/// CHARACTER length is the column `charcol('.') - 1` must report when the insert
/// cursor sits one past the last character.
const DOC_LINE_2: &str = "alpha";

/// A SECOND plain Markdown document, used only by the background-`bufload` guard
/// test. Its content differs from [`DOC`] purely so a failure message is unambiguous
/// about which of the two files it is describing.
const OTHER_DOC: &str = "# Other\ndelta\necho\n";

/// A document whose third line is three `é` (2 bytes each in UTF-8) followed by
/// `X`. Line 1 is a heading, line 2 is blank, line 3 is the multibyte line.
const MULTIBYTE_DOC: &str = "# T\n\néééX\n";

/// The CHARACTER column reached by three `l` motions from column 0 on
/// `MULTIBYTE_DOC`'s line 3.
const MULTIBYTE_CHAR_COL: u32 = 3;

/// The BYTE column those same three characters occupy (3 x 2 bytes). If the Lua
/// side regressed from `charcol('.')` to `col('.')` this is what would be reported,
/// so the multibyte test asserts against it explicitly.
const MULTIBYTE_BYTE_COL: u32 = 6;

/// The 1-based line of `MULTIBYTE_DOC`'s multibyte line.
const MULTIBYTE_LINE: u32 = 3;

/// How long to poll for a `markdown_cursors` condition before failing loudly.
/// The notification arrives independently of redraw flushes and nvim startup is not
/// instantaneous, so we wait well beyond the expected latency.
const CURSOR_TIMEOUT: Duration = FLUSH_TIMEOUT;

/// Gap between `read_snapshot` polls while waiting for a condition to settle.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// How long to let the out-of-band notifications settle before asserting that
/// something did NOT happen (no key appeared, no tick advanced). Generous so a late
/// emit cannot sneak a false positive past a NEGATIVE assertion.
const NEGATIVE_SETTLE: Duration = Duration::from_secs(2);

/// Polls `core`'s snapshot until `probe` yields `Some`, returning that value.
/// Drains pending wakes each iteration so the wake channel never backs up, and
/// panics after [`CURSOR_TIMEOUT`] with the last observed cursor/tick/preview maps
/// so a failure is diagnosable rather than a bare timeout.
fn poll_snapshot<T>(
    core: &WolfCore,
    waiter: &FlushWaiter,
    desc: &str,
    probe: impl Fn(&Snapshot) -> Option<T>,
) -> T {
    let deadline = Instant::now() + CURSOR_TIMEOUT;
    loop {
        {
            let snap = core.read_snapshot();
            if let Some(v) = probe(&snap) {
                return v;
            }
        }
        if Instant::now() >= deadline {
            let snap = core.read_snapshot();
            panic!(
                "timed out after {CURSOR_TIMEOUT:?} waiting for {desc}; last observed: \
                 markdown_cursors={:?}, markdown_ticks={:?}, previews={:?}, \
                 active_win={:?}, win_buffers={:?}",
                snap.markdown_cursors,
                snap.markdown_ticks,
                snap.previews,
                snap.active_win,
                snap.win_buffers
            );
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Resolves the Neovim buffer NUMBER of the buffer showing `path`, by polling the
/// per-window `win_buffers` map (fed by the `wolf_buffers` notification) for an
/// entry whose canonicalized path matches.
///
/// Both sides are canonicalized: `/tmp` and `/var` are symlinks on macOS and core
/// canonicalizes buffer paths, so a raw comparison would never match.
fn wait_for_source_bufnr(core: &WolfCore, waiter: &FlushWaiter, path: &Path) -> i64 {
    let want = std::fs::canonicalize(path).expect("the opened .md must exist and canonicalize");
    poll_snapshot(
        core,
        waiter,
        "win_buffers to report the opened .md's buffer number",
        |snap| {
            snap.win_buffers
                .values()
                .find(|wb| wb.path.as_deref() == Some(want.as_path()))
                .map(|wb| wb.bufnr)
        },
    )
}

/// Polls the ACTIVE window's buffer number (via `active_win` + `win_buffers`) until
/// it satisfies `pred`, and returns it.
///
/// Preview scratch panes are EXCLUDED from `win_buffers`, so while a preview pane is
/// active this never resolves — which is exactly the property the preview-guard test
/// relies on when it waits to be back in the source pane.
fn wait_for_active_bufnr(
    core: &WolfCore,
    waiter: &FlushWaiter,
    desc: &str,
    pred: impl Fn(i64) -> bool,
) -> i64 {
    poll_snapshot(core, waiter, desc, |snap| {
        let win = snap.active_win?;
        let bufnr = snap.win_buffers.get(&win)?.bufnr;
        pred(bufnr).then_some(bufnr)
    })
}

/// Polls `markdown_cursors[bufnr]` until it exists AND satisfies `pred`, returning
/// the matching [`MarkdownCursor`].
fn wait_for_cursor(
    core: &WolfCore,
    waiter: &FlushWaiter,
    bufnr: i64,
    desc: &str,
    pred: impl Fn(MarkdownCursor) -> bool,
) -> MarkdownCursor {
    poll_snapshot(core, waiter, desc, |snap| {
        snap.markdown_cursors
            .get(&bufnr)
            .copied()
            .filter(|c| pred(*c))
    })
}

/// The current key set of `markdown_cursors`, for before/after comparisons in the
/// NEGATIVE tests (no new key may appear for a non-markdown / preview buffer).
fn cursor_keys(core: &WolfCore) -> HashSet<i64> {
    core.read_snapshot()
        .markdown_cursors
        .keys()
        .copied()
        .collect()
}

/// Types an ex command as raw keystrokes: `:` enters command-line mode, `cmd` is
/// typed literally, Enter executes it (the same mechanism `current_file.rs` uses for
/// its raw `:enew`).
fn ex(core: &WolfCore, cmd: &str) {
    core.send_input(ch(':'));
    type_str(core, cmd);
    core.send_input(key(Key::Enter));
}

/// Burns [`NEGATIVE_SETTLE`] while draining wakes, so a NEGATIVE assertion made
/// afterwards cannot be beaten by a late notification.
fn settle(waiter: &FlushWaiter) {
    let deadline = Instant::now() + NEGATIVE_SETTLE;
    while Instant::now() < deadline {
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Sends `gg0`: jump to line 1, then to column 0. Normalizes the cursor before a
/// motion sequence so the asserted line/column values are absolute, not relative to
/// wherever nvim happened to leave the cursor after the open.
fn normalize_cursor(core: &WolfCore) {
    core.send_input(ch('g'));
    core.send_input(ch('g'));
    core.send_input(ch('0'));
}

/// The ground-truth contract: `line` is 1-BASED and `col` is 0-BASED.
///
/// Opens a plain multi-line `.md`, normalizes the cursor to line 1 / column 0, then
/// sends a `j` (down one hard line) and an `l` (right one character) and asserts the
/// reported pair walks `(1, 0)` -> `(2, 0)` -> `(2, 1)`.
#[test]
fn cursor_reports_one_based_line_and_zero_based_char_col() {
    let (core, waiter, file) = common::start_core_with_temp_md(DOC);
    let bufnr = wait_for_source_bufnr(&core, &waiter, file.path());

    normalize_cursor(&core);
    let start = wait_for_cursor(
        &core,
        &waiter,
        bufnr,
        "the cursor to settle at line 1, column 0 after `gg0`",
        |c| c.line == 1 && c.col == 0,
    );
    assert_eq!(
        (start.line, start.col),
        (1, 0),
        "`gg0` must report the FIRST line as 1 (line() is 1-based) and the FIRST \
         column as 0 (charcol() - 1 is 0-based)"
    );

    // `j` moves down one hard line; the column is retained.
    core.send_input(ch('j'));
    let after_j = wait_for_cursor(
        &core,
        &waiter,
        bufnr,
        "the cursor line to advance to 2 after `j`",
        |c| c.line == 2,
    );
    assert_eq!(
        after_j.line, 2,
        "a single `j` from line 1 must report line 2 (1-based, +1 per hard line)"
    );
    assert_eq!(
        after_j.col, 0,
        "`j` retains the column, so the reported 0-based column stays 0"
    );

    // `l` moves right one CHARACTER.
    core.send_input(ch('l'));
    let after_l = wait_for_cursor(
        &core,
        &waiter,
        bufnr,
        "the cursor column to advance to 1 after `l`",
        |c| c.line == 2 && c.col == 1,
    );
    assert_eq!(
        (after_l.line, after_l.col),
        (2, 1),
        "one `l` from column 0 must report the 0-BASED column 1 (not the 1-based 2)"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// FRESHNESS: a markdown buffer that is merely OPENED — and never touched — must
/// ALREADY report a cursor.
///
/// This pins why the cursor autocmd list is not just `CursorMoved`/`CursorMovedI`.
/// Probed against the pinned nvim, `:edit foo.md` fires `CursorMoved` BEFORE
/// filetype detection has run, so at that moment `filetype` is still `""` and
/// `emit_cursor`'s markdown guard bails — and no FURTHER `CursorMoved` fires until
/// the user actually moves. With only the motion events, a markdown buffer that is
/// opened and simply LOOKED at would report no cursor at all, so a preview bound to
/// it would sit at the `line == 0` "cursor unknown" sentinel — which the front-end
/// deliberately refuses to scroll for — until the first keystroke. That is a real
/// feature gap, not a test artifact. `BufEnter`/`BufWinEnter` both fire AFTER
/// detection, with the buffer current and its filetype set, and close it.
///
/// So this test sends **ZERO** keystrokes. `start_core_with_temp_md` opens the file
/// with `InputEvent::Open` (a plain `:edit <path>`) and does nothing else; the
/// `markdown_cursors` entry must exist purely from that. Dropping
/// `BufEnter`/`BufWinEnter` from the autocmd list makes this time out.
#[test]
fn cursor_reported_on_buffer_entry_without_any_motion() {
    let (core, waiter, file) = common::start_core_with_temp_md(DOC);
    let bufnr = wait_for_source_bufnr(&core, &waiter, file.path());

    // NOTE: not one `send_input` appears anywhere in this test, by design.
    let cursor = wait_for_cursor(
        &core,
        &waiter,
        bufnr,
        "a freshly opened, untouched .md to report a cursor with NO keystrokes sent",
        |c| c.line >= 1,
    );
    assert!(
        cursor.line >= 1,
        "an opened-but-untouched markdown buffer must report a REAL 1-based line, \
         never the `line == 0` cursor-unknown sentinel — otherwise a preview opened \
         on it has no anchor until the user happens to move; got {cursor:?}"
    );
    // The fixture is a freshly created temp path, so no shada mark can exist for it
    // and nvim always lands on line 1, column 0.
    assert_eq!(
        (cursor.line, cursor.col),
        (1, 0),
        "the reported position must describe the JUST-OPENED buffer at its initial \
         line 1 / column 0, not a stale or default value; got {cursor:?}"
    );

    let keys = cursor_keys(&core);
    assert!(
        keys.contains(&bufnr),
        "the opened source buffer (bufnr {bufnr}) must be a markdown_cursors key \
         without any motion; keys were {keys:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// THE load-bearing OD4 test: the reported column is a CHARACTER column, never a
/// BYTE column.
///
/// Line 3 of [`MULTIBYTE_DOC`] is `éééX`; each `é` is 2 bytes in UTF-8. Three `l`
/// motions from column 0 move three CHARACTERS, so `charcol('.') - 1` must report
/// `3`. A regression to `col('.') - 1` would report the byte column `6`, which is
/// asserted against explicitly so the failure names the bug rather than reading as a
/// vague off-by-N.
#[test]
fn multibyte_line_reports_character_column_not_byte_column() {
    let (core, waiter, file) = common::start_core_with_temp_md(MULTIBYTE_DOC);
    let bufnr = wait_for_source_bufnr(&core, &waiter, file.path());

    // Jump to the multibyte line (`3G`), then to its first column (`0`).
    core.send_input(ch('3'));
    core.send_input(ch('G'));
    core.send_input(ch('0'));
    wait_for_cursor(
        &core,
        &waiter,
        bufnr,
        "the cursor to reach the multibyte line at column 0",
        |c| c.line == MULTIBYTE_LINE && c.col == 0,
    );

    // Three `l` motions == three CHARACTERS (nvim's `l` is character-wise).
    core.send_input(ch('l'));
    core.send_input(ch('l'));
    core.send_input(ch('l'));

    let cursor = wait_for_cursor(
        &core,
        &waiter,
        bufnr,
        "the cursor column to reach the 3rd CHARACTER of the multibyte line",
        |c| c.line == MULTIBYTE_LINE && c.col == MULTIBYTE_CHAR_COL,
    );
    assert_eq!(
        cursor.col, MULTIBYTE_CHAR_COL,
        "three `l` motions over three 2-byte `é` characters must report the CHARACTER \
         column {MULTIBYTE_CHAR_COL}"
    );
    assert_ne!(
        cursor.col, MULTIBYTE_BYTE_COL,
        "the reported column must NOT be the BYTE column {MULTIBYTE_BYTE_COL} — that \
         means the Lua side regressed from `charcol('.')` to `col('.')`, which would \
         desync the `wl-{{L}}-{{C}}` marker lookup for every non-ASCII line"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Pins `charcol()`'s past-end-of-line insert-mode semantics (plan §11 risk 1).
///
/// In insert mode the cursor may sit ONE PAST the last character, where
/// `charcol('.')` returns `char_len + 1`, so the reported 0-based column equals the
/// line's CHARACTER LENGTH — a column no marker can ever exist at. This is benign
/// (the scroll JS rounds the column down to a multiple of 100 and falls back to the
/// line marker), but it must be pinned rather than assumed, because a future
/// "defensive clamp to the last character" would silently change the contract.
#[test]
fn insert_mode_past_eol_reports_the_character_length_column() {
    let (core, waiter, file) = common::start_core_with_temp_md(DOC);
    let bufnr = wait_for_source_bufnr(&core, &waiter, file.path());

    // Park on line 2 ("alpha") at column 0.
    normalize_cursor(&core);
    core.send_input(ch('j'));
    wait_for_cursor(
        &core,
        &waiter,
        bufnr,
        "the cursor to reach line 2 before entering insert mode",
        |c| c.line == 2,
    );

    // `A` appends AFTER the last character: insert mode, cursor one past EOL.
    core.send_input(ch('A'));

    let eol_col = u32::try_from(DOC_LINE_2.chars().count())
        .expect("the fixture line length must fit in a u32");
    let cursor = wait_for_cursor(
        &core,
        &waiter,
        bufnr,
        "the insert-mode cursor to report the past-EOL column",
        |c| c.line == 2 && c.col == eol_col,
    );
    assert_eq!(
        cursor.col, eol_col,
        "in insert mode past EOL `charcol('.')` returns char_len + 1, so the reported \
         0-based column must equal the line's CHARACTER length ({eol_col}) — one past \
         the last character"
    );

    core.send_input(key(Key::Esc));
    core.shutdown().expect("shutdown should be Ok");
}

/// The cursor signal is INDEPENDENT of the dirty signal: a pure normal-mode motion
/// updates `markdown_cursors` and leaves `markdown_ticks` untouched.
///
/// An edit is made first so there IS a recorded tick to pin (an untouched buffer has
/// no `markdown_ticks` entry at all, which would make the "unchanged" assertion
/// vacuous).
#[test]
fn cursor_updates_without_an_edit() {
    let (core, waiter, file) = common::start_core_with_temp_md(DOC);
    let bufnr = wait_for_source_bufnr(&core, &waiter, file.path());

    // Edit once so `wolf_markdown_dirty` has fired and a tick is recorded.
    core.send_input(ch('i'));
    core.send_input(ch('x'));
    core.send_input(key(Key::Esc));
    poll_snapshot(
        &core,
        &waiter,
        "markdown_ticks to record a changedtick for the edited source buffer",
        |snap| snap.markdown_ticks.get(&bufnr).copied(),
    );

    // Let any trailing TextChanged emits land before baselining the tick.
    settle(&waiter);
    let tick_before = core
        .read_snapshot()
        .markdown_ticks
        .get(&bufnr)
        .copied()
        .expect("the edited source buffer must have a recorded changedtick");

    normalize_cursor(&core);
    let cursor_before = wait_for_cursor(
        &core,
        &waiter,
        bufnr,
        "the cursor to settle at line 1 before the pure motion",
        |c| c.line == 1 && c.col == 0,
    );

    // A PURE normal-mode motion: `j` fires CursorMoved but NOT TextChanged.
    core.send_input(ch('j'));
    let cursor_after = wait_for_cursor(
        &core,
        &waiter,
        bufnr,
        "the cursor to move to line 2 on a pure normal-mode motion",
        |c| c.line == 2,
    );
    assert_ne!(
        cursor_before, cursor_after,
        "a pure normal-mode motion must UPDATE markdown_cursors"
    );

    // Give a (hypothetical) spurious dirty emit every chance to land, then assert.
    settle(&waiter);
    let tick_after = core
        .read_snapshot()
        .markdown_ticks
        .get(&bufnr)
        .copied()
        .expect("the source buffer must still have a recorded changedtick");
    assert_eq!(
        tick_after, tick_before,
        "a pure normal-mode motion must NOT bump markdown_ticks — the cursor signal \
         and the dirty signal are independent, so a cursor move must never arm the \
         front-end's HTML-reload coalescer"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// The filetype guard: a NON-markdown buffer never becomes a `markdown_cursors` key.
///
/// The startup buffer is a markdown-named `Untitled.md`, so a genuinely non-markdown
/// buffer is created AFTER startup with a raw `:enew` typed on the nvim command line
/// (it does not route through `wolf_new_untitled`, so it stays unnamed with an empty
/// filetype). Text is then typed and the cursor moved inside it — firing
/// `CursorMovedI` and `CursorMoved` for that buffer — and the key set must be
/// unchanged.
#[test]
fn non_markdown_buffer_reports_no_cursor() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    // Establish that the pipeline IS live: type into the markdown startup buffer so
    // CursorMovedI fires for it and it gains a cursor entry.
    let md_bufnr = wait_for_active_bufnr(
        &core,
        &waiter,
        "the markdown startup buffer to be the active window's buffer",
        |b| b > 0,
    );
    core.send_input(ch('i'));
    type_str(&core, "alpha bravo");
    core.send_input(key(Key::Esc));
    wait_for_cursor(
        &core,
        &waiter,
        md_bufnr,
        "the markdown startup buffer to report a cursor",
        |c| c.line >= 1,
    );
    let before = cursor_keys(&core);
    assert!(
        before.contains(&md_bufnr),
        "sanity: the markdown startup buffer must be a markdown_cursors key before \
         the non-markdown leg; keys were {before:?}"
    );

    // A genuinely unnamed, filetype-less buffer.
    ex(&core, "enew");
    let plain_bufnr = wait_for_active_bufnr(
        &core,
        &waiter,
        "the raw `:enew` buffer to become the active window's buffer",
        |b| b > 0 && b != md_bufnr,
    );

    // Move the cursor in it, in BOTH modes (insert typing fires CursorMovedI; the
    // normal-mode motions fire CursorMoved).
    core.send_input(ch('i'));
    type_str(&core, "plain text, not markdown");
    core.send_input(key(Key::Esc));
    core.send_input(ch('0'));
    core.send_input(ch('l'));
    core.send_input(ch('l'));
    core.send_input(ch('$'));

    settle(&waiter);
    let after = cursor_keys(&core);
    assert!(
        !after.contains(&plain_bufnr),
        "a NON-markdown buffer (bufnr {plain_bufnr}) must never become a \
         markdown_cursors key — the emit's `filetype == \"markdown\"` guard is what \
         keeps every plain-text keystroke off the notification channel; keys were \
         {after:?}"
    );
    assert_eq!(
        after, before,
        "no NEW markdown_cursors key may appear while editing a non-markdown buffer"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// The `b:wolf_preview_source` guard: a PREVIEW SCRATCH buffer never becomes a
/// `markdown_cursors` key, even when its filetype IS markdown.
///
/// A typed `:vsplit` first gives the source a second window; `wolf_open_preview()`
/// then binds the preview scratch IN PLACE into the active (new split) pane,
/// leaving that PREVIEW pane current. The scratch is then deliberately made
/// `modifiable` and given `filetype=markdown`, which defeats the INCIDENTAL exclusion
/// the sibling `wolf_markdown_dirty` autocmd relies on (a scratch never runs filetype
/// detection, so its filetype is normally `""`). That leaves the EXPLICIT
/// `b:wolf_preview_source` guard as the only thing standing between the scratch's
/// cursor moves and the notification — which is exactly what this test pins.
///
/// The final leg hops back to the source pane and moves the cursor there, proving the
/// pipeline was alive the whole time so the absence of the preview key is meaningful
/// rather than vacuous.
#[test]
fn preview_scratch_buffer_reports_no_cursor() {
    let (core, waiter, file) = common::start_core_with_temp_md(DOC);
    let source_bufnr = wait_for_source_bufnr(&core, &waiter, file.path());

    normalize_cursor(&core);
    wait_for_cursor(
        &core,
        &waiter,
        source_bufnr,
        "the SOURCE buffer to report a cursor before the preview is opened",
        |c| c.line == 1,
    );

    // A second pane first (typed `:vsplit`), so the SOURCE keeps a window of its
    // own once the in-place open_preview() converts the active pane to a preview.
    ex(&core, "vsplit");
    poll_snapshot(&core, &waiter, "two live windows after :vsplit", |snap| {
        (snap.windows.len() >= 2).then_some(())
    });

    let preview_bufnr = core.open_preview();
    assert!(
        preview_bufnr > 0,
        "open_preview() on a markdown buffer must return the preview scratch bufnr \
         (> 0), got {preview_bufnr}"
    );
    assert_ne!(
        preview_bufnr, source_bufnr,
        "the preview scratch must be a DIFFERENT buffer from its source"
    );

    poll_snapshot(&core, &waiter, "a preview window to appear", |snap| {
        (!snap.previews.is_empty()).then_some(())
    });
    poll_snapshot(
        &core,
        &waiter,
        "the in-place-bound PREVIEW pane to become the active pane",
        |snap| snap.is_preview_active().then_some(()),
    );

    // Defeat the incidental empty-filetype exclusion so the EXPLICIT preview guard is
    // the only thing under test, and make the scratch typable so motions really move.
    ex(&core, "setlocal modifiable");
    ex(&core, "setlocal filetype=markdown");
    core.send_input(ch('i'));
    type_str(&core, "typed into the preview scratch");
    core.send_input(key(Key::Esc));
    core.send_input(ch('0'));
    core.send_input(ch('l'));
    core.send_input(ch('l'));
    core.send_input(ch('$'));

    settle(&waiter);
    // Snapshot the key set ONCE (never hold two read guards across one assert).
    let keys_in_preview = cursor_keys(&core);
    assert!(
        !keys_in_preview.contains(&preview_bufnr),
        "the PREVIEW SCRATCH buffer (bufnr {preview_bufnr}) must never become a \
         markdown_cursors key — only SOURCE buffers are keys, so the `wl-{{L}}-{{C}}` \
         anchor a preview scrolls to always describes its own source; keys were \
         {keys_in_preview:?}"
    );

    // Hop back to the SOURCE pane and move: proves the cursor pipeline is still live,
    // so the absence above is a real guard and not a dead notification channel.
    ex(&core, "wincmd w");
    wait_for_active_bufnr(
        &core,
        &waiter,
        "the SOURCE pane to become active again after `:wincmd w`",
        |b| b == source_bufnr,
    );
    core.send_input(ch('j'));
    wait_for_cursor(
        &core,
        &waiter,
        source_bufnr,
        "the SOURCE buffer's cursor to move again after returning to its pane",
        |c| c.line == 2,
    );

    let keys_after_return = cursor_keys(&core);
    assert!(
        !keys_after_return.contains(&preview_bufnr),
        "the preview scratch must STILL not be a markdown_cursors key after the \
         source's cursor started moving again; keys were {keys_after_return:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// The `win_gettype()` guard: a markdown file loaded in the BACKGROUND — with no
/// window of its own — must never become a `markdown_cursors` key.
///
/// **The measured fact this pins, and the whole reason the guard exists:**
/// `vim.fn.bufload()` fires `BufEnter` **and** `BufWinEnter` for the target buffer
/// inside nvim's synthetic **autocmd window**, with that buffer GENUINELY current.
/// Probed against the pinned nvim 0.12.3, that window reports
/// `win_gettype() == "autocmd"` and carries a fabricated line 1 / column 1 cursor
/// belonging to no user pane. Because the buffer really is current, really is a
/// loaded markdown file (`bufload` runs filetype detection), and really has no
/// `b:wolf_preview_source`, this defeats `emit_cursor`'s
/// `buf ~= nvim_get_current_buf()` guard, its `filetype == "markdown"` guard and its
/// preview-scratch guard ALL AT ONCE — three guards, not one of which can see the
/// difference. Only `win_gettype() ~= ""` can: a NORMAL window is the only kind that
/// carries a real cursor.
///
/// Without that guard every background-loaded source — which is exactly what
/// `wolf_preview_file` produces for a link-navigated preview — would report a bogus
/// `(1, 0)` top-of-document anchor instead of the `line == 0` "cursor unknown"
/// sentinel the front-end relies on to leave a freshly bound preview's scroll
/// position alone, and it would fight the `#anchor` fragment jump.
///
/// This is currently pinned only INDIRECTLY, through the whole FFI layer, by
/// `crates/wolf-ffi/tests/preview.rs`'s
/// `preview_cursor_defaults_to_zero_when_unreported`. Here it is pinned at the
/// Lua↔core seam where the guard actually lives.
///
/// **Why the negative assertion is not vacuous:**
///  * `markdown_cursors` is UPSERT-ONLY (`apply_markdown_cursor` performs a single
///    `insert`; nothing anywhere removes a key), so ONE key-set read taken after
///    [`settle`] genuinely means "no key EVER appeared", not merely "none is present
///    at this instant".
///  * [`settle`] first burns [`NEGATIVE_SETTLE`] draining wakes, so a late emit
///    cannot slip in behind the read.
///  * The focused buffer's cursor is asserted to keep MOVING afterwards, so the
///    notification channel is demonstrably alive across the whole window.
///  * The background file is finally `:edit`ed into the real window and MUST then
///    report a cursor — which both names the exact bufnr that had to stay absent and
///    proves the guard rejected the synthetic autocmd WINDOW, not this buffer or
///    this file.
#[test]
fn background_bufload_of_a_markdown_file_reports_no_cursor() {
    let (core, waiter, file) = common::start_core_with_temp_md(DOC);
    let source_bufnr = wait_for_source_bufnr(&core, &waiter, file.path());

    // A second, real markdown file on disk. Kept alive for the test's duration.
    let other = common::temp_md(OTHER_DOC);
    let other_path = other
        .path()
        .to_str()
        .expect("the temp .md path must be valid UTF-8 to type as an ex command")
        .to_owned();

    normalize_cursor(&core);
    let source_before = wait_for_cursor(
        &core,
        &waiter,
        source_bufnr,
        "the FOCUSED source buffer's cursor to settle before the background load",
        |c| c.line == 1 && c.col == 0,
    );
    let keys_before = cursor_keys(&core);
    assert!(
        keys_before.contains(&source_bufnr),
        "sanity: the focused source buffer (bufnr {source_bufnr}) must already be a \
         markdown_cursors key before the background load; keys were {keys_before:?}"
    );

    // Load in the background the way `wolf_preview_file` does; `bufadd('…')` names the existing
    // buffer and keeps the path in a string, immune to nvim's `%`/`#` file-argument expansion.
    ex(&core, &format!("badd {other_path}"));
    ex(&core, &format!("call bufload(bufadd('{other_path}'))"));

    settle(&waiter);

    let keys_after = cursor_keys(&core);
    assert_eq!(
        keys_after, keys_before,
        "a BACKGROUND bufload must add NO markdown_cursors key. bufload fires \
         BufEnter/BufWinEnter inside nvim's synthetic AUTOCMD WINDOW with the loaded \
         buffer genuinely current and its markdown filetype already detected, so the \
         current-buffer, filetype and preview-scratch guards ALL pass there — only \
         emit_cursor's `win_gettype() == \"\"` guard can reject it. A new key here \
         means a buffer with no window just fabricated a (1, 0) anchor. Keys before \
         {keys_before:?}, after {keys_after:?}"
    );

    let source_after = core
        .read_snapshot()
        .markdown_cursors
        .get(&source_bufnr)
        .copied()
        .expect("the focused source buffer must still have a cursor entry");
    assert_eq!(
        source_after, source_before,
        "the FOCUSED buffer's own entry must be UNCHANGED across the background load \
         — the autocmd window's fabricated line 1 / column 1 cursor must not leak \
         into any entry, not even the current buffer's (the autocmd window is entered \
         and left again around the load)"
    );

    // The notification channel is demonstrably still alive after the background
    // load, so the absence asserted above is a real guard, not a dead channel.
    core.send_input(ch('j'));
    wait_for_cursor(
        &core,
        &waiter,
        source_bufnr,
        "the FOCUSED buffer's cursor to keep moving after the background load",
        |c| c.line == 2,
    );

    // Bringing the same buffer into a normal window, which carries a real cursor, reveals the
    // bufnr that had to stay absent, and it must now report one.
    ex(&core, &format!("edit {other_path}"));
    let other_bufnr = wait_for_source_bufnr(&core, &waiter, other.path());
    assert_ne!(
        other_bufnr, source_bufnr,
        "the background-loaded file must be a DIFFERENT buffer from the focused one, \
         otherwise this test is asserting about the wrong buffer entirely"
    );
    assert!(
        !keys_after.contains(&other_bufnr),
        "the background-loaded markdown buffer (bufnr {other_bufnr}) must never have \
         become a markdown_cursors key while it had no window of its own; the keys \
         observed after the load were {keys_after:?}"
    );
    let other_cursor = wait_for_cursor(
        &core,
        &waiter,
        other_bufnr,
        "the background-loaded buffer to report a cursor ONCE it is shown in a NORMAL \
         window",
        |c| c.line >= 1,
    );
    assert!(
        other_cursor.line >= 1,
        "the same buffer MUST report a cursor once a normal window shows it — that is \
         what proves the win_gettype() guard rejects the synthetic AUTOCMD WINDOW and \
         not this buffer or this file; got {other_cursor:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// `markdown_cursors` and `markdown_ticks` are keyed by the SAME SOURCE buffer
/// number, and that number is `PreviewInfo::source_bufnr` — the three-way join the
/// FFI's `previews()` accessor pair depends on.
///
/// Every buffer that reports a TICK must also report a CURSOR under the identical
/// key; the converse does not hold (an unedited markdown buffer has a cursor but no
/// tick), so the key relation asserted is a SUBSET, not equality.
#[test]
fn cursor_and_tick_share_the_source_bufnr_key() {
    let (core, waiter, file) = common::start_core_with_temp_md(DOC);
    let source_bufnr = wait_for_source_bufnr(&core, &waiter, file.path());

    // Fire wolf_markdown_dirty (the tick) and wolf_markdown_cursor (the position).
    core.send_input(ch('i'));
    core.send_input(ch('x'));
    core.send_input(key(Key::Esc));
    poll_snapshot(
        &core,
        &waiter,
        "markdown_ticks to record a changedtick for the edited source buffer",
        |snap| snap.markdown_ticks.get(&source_bufnr).copied(),
    );
    wait_for_cursor(
        &core,
        &waiter,
        source_bufnr,
        "markdown_cursors to record a position for the edited source buffer",
        |c| c.line >= 1,
    );

    // Open the preview in place so `previews` carries the same source bufnr as both maps; the maps
    // are upsert-only, so losing the source's window does not disturb the keys.
    let preview_bufnr = core.open_preview();
    assert!(
        preview_bufnr > 0,
        "open_preview() must return the preview scratch bufnr, got {preview_bufnr}"
    );
    let preview_source_bufnrs = poll_snapshot(
        &core,
        &waiter,
        "a preview window carrying its source bufnr to appear",
        |snap| {
            let bufnrs: Vec<i64> = snap.previews.values().map(|p| p.source_bufnr).collect();
            (!bufnrs.is_empty()).then_some(bufnrs)
        },
    );
    assert!(
        preview_source_bufnrs.contains(&source_bufnr),
        "PreviewInfo::source_bufnr must be the SOURCE buffer's number ({source_bufnr}); \
         got {preview_source_bufnrs:?}"
    );

    // Copy both key sets out under ONE read guard, then assert on the copies.
    let (tick_key_set, cursor_key_set): (HashSet<i64>, HashSet<i64>) = {
        let snap = core.read_snapshot();
        (
            snap.markdown_ticks.keys().copied().collect(),
            snap.markdown_cursors.keys().copied().collect(),
        )
    };
    assert!(
        tick_key_set.contains(&source_bufnr),
        "markdown_ticks must be keyed by the SOURCE bufnr {source_bufnr}; keys were \
         {tick_key_set:?}"
    );
    assert!(
        cursor_key_set.contains(&source_bufnr),
        "markdown_cursors must be keyed by the SAME SOURCE bufnr {source_bufnr} so the \
         FFI `previews()` join can hand each preview window its source's cursor; keys \
         were {cursor_key_set:?}"
    );
    assert!(
        tick_key_set.is_subset(&cursor_key_set),
        "every buffer that reports a TICK must report a CURSOR under the IDENTICAL key \
         (both maps are keyed by the SOURCE bufnr); tick keys {tick_key_set:?} are not \
         a subset of cursor keys {cursor_key_set:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}
