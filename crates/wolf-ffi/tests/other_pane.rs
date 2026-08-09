//! Integration test for the `wolf-ffi` OTHER-PANE bridge surface: the
//! "Preview in Another Pane" / "Edit in Another Pane" command pair. Where
//! `open_preview()` / `edit_source()` act IN PLACE on the active pane, this pair
//! targets ANOTHER pane: reuse the given target window when it is valid, else
//! split — an atomic split+bind performed inside Lua (`wolf_do_split("v")` plus a
//! bind), surfacing to the front-end as an ordinary vertical Split layout intent.
//!
//! The focus rule under test: **focus follows the edit-mode view of the
//! document**. Preview-in-other keeps focus in the pane EDITING the source (the
//! preview lands unfocused elsewhere); Edit-in-other moves focus to the pane now
//! editing the source.
//!
//! This drives the RUST half against a REAL, hermetic Neovim — NO mocks, per the
//! project testing policy. It is modeled on `tests/preview.rs` / `tests/edit.rs`
//! (helpers, poll idiom) and `tests/panes.rs` (the Split layout-intent drain),
//! reusing `tests/common/mod.rs` and the same loud SKIP-when-no-nvim guard.
//!
//! ## Dependency on coder work (other-pane plan D2)
//!
//! The surface these tests compile against, on `WolfCoreHandle`:
//!   * `preview_in_other(&self, target_win: i64) -> i64` — previews the active
//!     markdown pane's file in `target_win` (0 / invalid → split right), returning
//!     the SOURCE bufnr or the neutral 0 (gate failed / shut down)
//!   * `edit_in_other(&self, target_win: i64) -> i64` — edits the active preview
//!     pane's source in `target_win` (0 / invalid → split, source lands LEFT),
//!     returning the bufnr bound into the target or the neutral 0
//! plus the `_G.wolf_preview_in_other` / `_G.wolf_edit_in_other` helpers in
//! `runtime/nvim/init.lua`. This file will not compile until that lands — expected.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim (`WOLF_NVIM_BIN` or the
//! Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is available.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiBufferRow, FfiKeyEvent, FfiPreview};

/// Generous overall timeout for nvim `--embed` startup plus each async effect
/// (filetype detection, the split+bind, the out-of-band `wolf_preview` /
/// `wolf_current_file` notifications) to settle. Mirrors the siblings.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface. Mirrors the siblings.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// How long to keep draining intents while asserting that a Split did NOT happen.
/// Generous, so a late out-of-band Split intent cannot sneak past the NEGATIVE
/// assertion. Mirrors `preview.rs`'s `NEGATIVE_SETTLE`.
const NEGATIVE_SETTLE: Duration = Duration::from_millis(1500);

/// The lowest real WINDOW grid id (grid 1 is the outer/global grid; editor windows
/// are `>= 2` under `ext_multigrid`). Mirrors `preview.rs`.
const WINDOW_GRID_MIN: u64 = 2;

/// The single-pane window count (the gate no-op cases must stay here).
const ONE_PANE: usize = 1;

/// The two-pane window count reached after a split (typed or helper-driven).
const TWO_PANES: usize = 2;

/// The three-window count reached when the split fallback fires while the
/// terminal drawer is also open (source pane + drawer + new split).
const THREE_WINDOWS: usize = 3;

/// A `max` large enough that the assembled buffer set is never truncated.
/// Mirrors `preview.rs` / `terminal.rs`.
const RANKED_MAX: usize = 100;

/// The Neovim naming convention prefix for a terminal buffer's name
/// (`term://…`), used to detect the drawer buffer in the picker snapshot.
/// Mirrors `terminal.rs`.
const TERM_PREFIX: &str = "term://";

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

// -- Layout-intent discriminants (mirror `panes.rs` / `mod intent_kind`) -------
/// `FfiLayoutIntent::kind()` for a window SPLIT.
const INTENT_KIND_SPLIT: u8 = 0;
/// `SplitOrientation::Vertical` discriminant (side-by-side, `:vsplit` /
/// `wolf_do_split("v")`).
const ORIENTATION_VERTICAL: u8 = 1;
/// `FfiLayoutIntent::kind()` for a DRAWER_OPEN intent (mirrors `terminal.rs` /
/// `mod intent_kind`): the `wolf_toggle_terminal` open path names the drawer
/// window through this intent.
const INTENT_KIND_DRAWER_OPEN: u8 = 2;

/// Builds a printable-character `FfiKeyEvent` (kind=Char) for `c`, all modifiers
/// false. Same as the sibling tests.
fn char_key(c: char) -> FfiKeyEvent {
    FfiKeyEvent {
        kind: KIND_CHAR,
        ch: c as u32,
        fnum: 0,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    }
}

/// Builds a CTRL-modified character `FfiKeyEvent` (kind=Char, `ctrl = true`) for
/// `c` — e.g. `ctrl_char_key('\\')` + `ctrl_char_key('n')` sends `<C-\><C-n>` to
/// leave terminal-insert mode. Copy of `terminal.rs`.
fn ctrl_char_key(c: char) -> FfiKeyEvent {
    FfiKeyEvent {
        kind: KIND_CHAR,
        ch: c as u32,
        fnum: 0,
        ctrl: true,
        shift: false,
        alt: false,
        meta: false,
    }
}

/// Builds an Enter `FfiKeyEvent` (kind=Enter), all modifiers false.
fn enter_key() -> FfiKeyEvent {
    FfiKeyEvent {
        kind: KIND_ENTER,
        ch: 0,
        fnum: 0,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    }
}

/// Types `cmd` as an Ex command and presses Enter, through the same
/// `send_key`/`FfiKeyEvent` path the generated Swift glue uses. `cmd` must include
/// the leading `:`. Copy of the siblings' `send_ex_command`.
fn send_ex_command(handle: &wolf_ffi::WolfCoreHandle, cmd: &str) {
    for c in cmd.chars() {
        handle.send_key(char_key(c));
    }
    handle.send_key(enter_key());
}

/// The live editor WINDOW grids `(grid, win)` (grids `>= WINDOW_GRID_MIN` that are
/// not hidden). Mirrors `panes.rs::live_windows`.
fn live_windows(handle: &wolf_ffi::WolfCoreHandle) -> Vec<(u64, i64)> {
    handle
        .windows()
        .iter()
        .filter(|w| !w.hidden() && w.grid() >= WINDOW_GRID_MIN)
        .map(|w| (w.grid(), w.win()))
        .collect()
}

/// The number of live editor window grids. Mirrors `preview.rs`.
fn live_window_count(handle: &wolf_ffi::WolfCoreHandle) -> usize {
    live_windows(handle).len()
}

/// Blocks (bounded by [`FLUSH_TIMEOUT`]) until the attached active frame reports a
/// non-empty grid (`cols > 0`), the sibling tests' attach handshake.
fn wait_for_attach(handle: &wolf_ffi::WolfCoreHandle) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut frame = handle.read_frame();
    while Instant::now() < deadline {
        frame = handle.read_frame();
        if frame.cols() > 0 {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        frame.cols() > 0,
        "core should attach and report a non-empty active frame (cols, got {})",
        frame.cols()
    );
}

/// Blocks until `handle.is_markdown_active()` equals `target`, then asserts it.
fn wait_for_markdown_active(handle: &wolf_ffi::WolfCoreHandle, target: bool, desc: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut got = handle.is_markdown_active();
    while Instant::now() < deadline {
        got = handle.is_markdown_active();
        if got == target {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        got, target,
        "is_markdown_active() should be {target} ({desc})"
    );
}

/// Blocks until `handle.is_preview_active()` equals `target`, then asserts it.
fn wait_for_preview_active(handle: &wolf_ffi::WolfCoreHandle, target: bool, desc: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut got = handle.is_preview_active();
    while Instant::now() < deadline {
        got = handle.is_preview_active();
        if got == target {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(
        got, target,
        "is_preview_active() should be {target} ({desc})"
    );
}

/// Blocks (bounded by [`FLUSH_TIMEOUT`]) until the core's `current_file()` ends
/// with `suffix` (a BASENAME match, robust to absolute-vs-cwd-relative display and
/// to `/var`<->`/private/var` symlinking), deterministically confirming that file
/// is the ACTIVE buffer. Copy of `preview.rs`'s helper.
fn wait_for_current_file(handle: &wolf_ffi::WolfCoreHandle, suffix: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut current = handle.current_file();
    while Instant::now() < deadline {
        current = handle.current_file();
        if current.ends_with(suffix) {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    panic!(
        "timed out after {FLUSH_TIMEOUT:?} waiting for current_file() to end with \
         {suffix} (got {current})"
    );
}

/// Blocks until exactly `n` live editor window grids exist, then asserts it.
fn wait_for_window_count(handle: &wolf_ffi::WolfCoreHandle, n: usize, desc: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut count = live_window_count(handle);
    while Instant::now() < deadline {
        count = live_window_count(handle);
        if count == n {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(count, n, "there should be {n} live window grids ({desc})");
}

/// Blocks until `handle.active_win()` equals `target`, then asserts it. Focus
/// rides the async notification path, so a synchronous read right after a
/// split+bind can be stale. Copy of `edit.rs`'s helper.
fn wait_for_active_win(handle: &wolf_ffi::WolfCoreHandle, target: i64, desc: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut got = handle.active_win();
    while Instant::now() < deadline {
        got = handle.active_win();
        if got == target {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    assert_eq!(got, target, "active_win() should be {target} ({desc})");
}

/// Blocks until `handle.active_win()` is nonzero, returning it.
fn wait_for_some_active_win(handle: &wolf_ffi::WolfCoreHandle) -> i64 {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        let got = handle.active_win();
        if got != 0 {
            return got;
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out after {FLUSH_TIMEOUT:?} waiting for a nonzero active_win()");
}

/// Blocks until `predicate(&previews)` holds on a fresh `previews()` snapshot,
/// returning that snapshot. Panics with `desc` on timeout. Same as `preview.rs`.
fn wait_for_previews(
    handle: &wolf_ffi::WolfCoreHandle,
    desc: &str,
    predicate: impl Fn(&[FfiPreview]) -> bool,
) -> Vec<FfiPreview> {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        let previews = handle.previews();
        if predicate(&previews) {
            return previews;
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out after {FLUSH_TIMEOUT:?} waiting for previews() ({desc})");
}

/// Polls `drain_intents()` until a SPLIT intent appears, returning its
/// `(orientation, from, win)` scalars. Draining accumulates across polls (each
/// call empties the queue). Copy of `panes.rs`'s drain idiom. Panics with `desc`
/// on timeout.
fn poll_for_split_intent(handle: &wolf_ffi::WolfCoreHandle, desc: &str) -> (u8, i64, i64) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut split: Option<(u8, i64, i64)> = None;
    while Instant::now() < deadline {
        for intent in handle.drain_intents() {
            if intent.kind() == INTENT_KIND_SPLIT {
                split = Some((intent.orientation(), intent.from(), intent.win()));
            }
        }
        if let Some(s) = split {
            return s;
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out after {FLUSH_TIMEOUT:?} waiting for a SPLIT layout intent ({desc})");
}

/// Polls `drain_intents()` until a DRAWER_OPEN intent appears, returning its
/// `win()` (the drawer window handle). Copy of `terminal.rs`'s drain idiom.
/// Panics with `desc` on timeout.
fn poll_for_drawer_open_intent(handle: &wolf_ffi::WolfCoreHandle, desc: &str) -> i64 {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut drawer_win: Option<i64> = None;
    while Instant::now() < deadline {
        for intent in handle.drain_intents() {
            if intent.kind() == INTENT_KIND_DRAWER_OPEN {
                drawer_win = Some(intent.win());
            }
        }
        if let Some(win) = drawer_win {
            return win;
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out after {FLUSH_TIMEOUT:?} waiting for a DRAWER_OPEN layout intent ({desc})");
}

/// A fresh buffer-picker snapshot ranked over the empty query (the picker
/// snapshots `list_buffers()` at construction, so every read constructs a NEW
/// picker). Mirrors `preview.rs`.
fn ranked_snapshot(handle: &wolf_ffi::WolfCoreHandle) -> Vec<FfiBufferRow> {
    handle.buffer_picker().ranked(String::new(), RANKED_MAX)
}

/// The number of ranked rows whose `display()` ends with `suffix` (robust to
/// cwd-relative vs. absolute display and `/var`<->`/private/var` symlinking).
/// Mirrors `preview.rs`.
fn rows_ending_with(rows: &[FfiBufferRow], suffix: &str) -> usize {
    rows.iter()
        .filter(|r| r.display().ends_with(suffix))
        .count()
}

/// Blocks (bounded by [`FLUSH_TIMEOUT`]) until `predicate(&rows)` holds on a
/// FRESH ranked picker snapshot, returning that snapshot. Panics with `desc` on
/// timeout. Mirrors `preview.rs`.
fn wait_for_ranked(
    handle: &wolf_ffi::WolfCoreHandle,
    desc: &str,
    predicate: impl Fn(&[FfiBufferRow]) -> bool,
) -> Vec<FfiBufferRow> {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        let rows = ranked_snapshot(handle);
        if predicate(&rows) {
            return rows;
        }
        sleep(POLL_INTERVAL);
    }
    panic!("timed out after {FLUSH_TIMEOUT:?} waiting for a ranked picker snapshot ({desc})");
}

/// The bufnrs of all ranked rows naming a terminal buffer (the `term://` prefix),
/// from a fresh picker snapshot. Mirrors `terminal.rs::term_bufnrs`.
fn term_bufnrs(handle: &wolf_ffi::WolfCoreHandle) -> Vec<i64> {
    ranked_snapshot(handle)
        .iter()
        .filter(|r| r.display().starts_with(TERM_PREFIX))
        .map(FfiBufferRow::bufnr)
        .collect()
}

/// Keeps draining intents for [`NEGATIVE_SETTLE`] and asserts NO Split intent
/// surfaces — the "gate rejected the call BEFORE any split" / "reuse never
/// splits" negative. The caller must have drained (or consumed) any Split intent
/// from earlier deliberate splits first.
fn assert_no_split_intent(handle: &wolf_ffi::WolfCoreHandle, desc: &str) {
    let deadline = Instant::now() + NEGATIVE_SETTLE;
    while Instant::now() < deadline {
        for intent in handle.drain_intents() {
            assert_ne!(
                intent.kind(),
                INTENT_KIND_SPLIT,
                "no Split layout intent may surface ({desc})"
            );
        }
        sleep(POLL_INTERVAL);
    }
}

/// Seeds a fresh temp cwd with a single real Markdown file `alpha.md`. Returns the
/// live `TempDir` (kept alive until after `shutdown()`). Copy of `edit.rs`.
fn seed_markdown_cwd() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    std::fs::write(
        tmp.path().join("alpha.md"),
        "# Alpha Heading\n\nSome paragraph text.\n",
    )
    .expect("failed to write alpha.md");
    tmp
}

/// Starts a core against `tmp` and blocks until attached. Argument order mirrors
/// the sibling tests exactly: (init_lua, cwd, cols, rows). Copy of `edit.rs`.
fn start_attached(tmp: &tempfile::TempDir) -> wolf_ffi::WolfCoreHandle {
    let init_lua = common::init_lua_path();
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");
    wait_for_attach(&handle);
    handle
}

/// Opens `alpha.md` as the deterministically-active buffer (the real-switch
/// barrier the siblings use: the startup buffer is a markdown `Untitled.md`, so
/// `wait_for_markdown_active(true)` alone cannot witness the `:e`).
fn open_alpha(handle: &wolf_ffi::WolfCoreHandle) {
    send_ex_command(handle, ":e alpha.md");
    wait_for_markdown_active(handle, true, "after :e alpha.md");
    wait_for_current_file(handle, "alpha.md");
}

#[test]
fn preview_in_other_splits_right_and_keeps_focus() {
    // Case 1 (split-preview) + case 7 (post-shutdown neutrals).
    //
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let tmp = seed_markdown_cwd();
    let handle = start_attached(&tmp);

    // --- 1. alpha.md active in the single editor pane ------------------------
    open_alpha(&handle);
    let from_win = wait_for_some_active_win(&handle);
    // Discard any startup intents so the Split observed below is OURS.
    let _ = handle.drain_intents();

    // --- 2. preview_in_other(0): no valid target -> split right + bind --------
    let source_bufnr = handle.preview_in_other(0);
    assert!(
        source_bufnr > 0,
        "preview_in_other(0) on a markdown pane must return the SOURCE bufnr \
         (> 0), got {source_bufnr}"
    );

    // The split surfaces as an ordinary VERTICAL Split intent from the original
    // window (applySplit inserts the new pane AFTER `from`, so it lands right).
    let (orientation, split_from, new_win) =
        poll_for_split_intent(&handle, "preview_in_other(0) split");
    assert_eq!(
        orientation, ORIENTATION_VERTICAL,
        "the preview_in_other split must be VERTICAL ({ORIENTATION_VERTICAL})"
    );
    assert_eq!(
        split_from, from_win,
        "the Split intent's from() must be the original (source) window"
    );
    assert_ne!(
        new_win, 0,
        "the Split intent's win() must be a nonzero new handle"
    );
    assert_ne!(
        new_win, from_win,
        "the Split intent's win() must be DISTINCT from the original window"
    );
    wait_for_window_count(&handle, TWO_PANES, "preview_in_other(0) splits");

    // --- 3. The preview is bound into the NEW window; source keeps its pane ---
    let previews = wait_for_previews(&handle, "one preview in the new window", |p| {
        p.len() == 1 && p.iter().any(|row| row.win() == new_win)
    });
    assert_eq!(
        previews[0].source_bufnr(),
        source_bufnr,
        "the preview row must carry the returned SOURCE bufnr"
    );

    // --- 4. Focus rule: focus follows the EDIT-mode view (the source editor) --
    wait_for_active_win(
        &handle,
        from_win,
        "preview_in_other keeps focus in the pane editing the source",
    );
    wait_for_markdown_active(
        &handle,
        true,
        "the focused pane still edits the markdown source",
    );
    wait_for_preview_active(
        &handle,
        false,
        "the focused pane is NOT the preview (it landed in the other pane)",
    );

    // --- 5. Shutdown: both calls return the neutral 0 -------------------------
    handle.shutdown();
    assert_eq!(
        handle.preview_in_other(0),
        0,
        "after shutdown preview_in_other() must be the neutral 0"
    );
    assert_eq!(
        handle.edit_in_other(0),
        0,
        "after shutdown edit_in_other() must be the neutral 0"
    );

    drop(tmp);
}

#[test]
fn edit_in_other_splits_and_moves_focus_to_the_source_editor() {
    // Case 2 (split-edit): from an in-place preview pane, edit_in_other(0) splits;
    // the scratch stays in the NEW (right) window, the SOURCE is bound into the
    // ORIGINAL (left) window, and focus follows the source editor.
    //
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let tmp = seed_markdown_cwd();
    let handle = start_attached(&tmp);

    // --- 1. alpha.md, then the in-place preview (active pane becomes preview) --
    open_alpha(&handle);
    let scratch_bufnr = handle.open_preview();
    assert!(
        scratch_bufnr > 0,
        "open_preview() on a markdown buffer must return a valid preview bufnr \
         (> 0), got {scratch_bufnr}"
    );
    wait_for_window_count(&handle, ONE_PANE, "in-place preview creates no pane");
    wait_for_preview_active(&handle, true, "the active pane is now the preview");
    let previews = wait_for_previews(&handle, "one alpha.md preview", |p| {
        p.len() == 1 && p.iter().any(|row| row.source_name() == "alpha.md")
    });
    let preview_win = previews[0].win();
    assert!(
        preview_win > 0,
        "the preview row must carry a real window handle (> 0), got {preview_win}"
    );
    let _ = handle.drain_intents();

    // --- 2. edit_in_other(0): split; scratch right, source left, focus left ----
    let returned = handle.edit_in_other(0);
    assert_ne!(
        returned, 0,
        "edit_in_other(0) on the active preview pane must return the bufnr bound \
         into the target window (nonzero)"
    );

    let (orientation, split_from, new_win) =
        poll_for_split_intent(&handle, "edit_in_other(0) split");
    assert_eq!(
        orientation, ORIENTATION_VERTICAL,
        "the edit_in_other split must be VERTICAL ({ORIENTATION_VERTICAL})"
    );
    assert_eq!(
        split_from, preview_win,
        "the Split intent's from() must be the (formerly preview) original window"
    );
    assert_ne!(new_win, preview_win, "the new window must be distinct");
    wait_for_window_count(&handle, TWO_PANES, "edit_in_other(0) splits");

    // The scratch stays in the NEW window: previews() re-emits with the single
    // preview row now carrying the new (right) window.
    wait_for_previews(&handle, "the scratch moved to the new window", |p| {
        p.len() == 1 && p.iter().any(|row| row.win() == new_win)
    });

    // --- 3. Focus rule: the ORIGINAL window now edits the source, focused ------
    wait_for_active_win(
        &handle,
        preview_win,
        "edit_in_other moves focus to the pane now EDITING the source",
    );
    wait_for_current_file(&handle, "alpha.md");
    wait_for_preview_active(
        &handle,
        false,
        "the focused pane edits the source, so is_preview_active falls",
    );
    wait_for_markdown_active(
        &handle,
        true,
        "the focused pane shows the markdown source again",
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn preview_in_other_reuses_an_existing_pane_without_splitting() {
    // Case 3 (reuse-preview): with a second editor pane already open, passing its
    // window reuses it — no new Split intent, no focus move.
    //
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let tmp = seed_markdown_cwd();
    let handle = start_attached(&tmp);

    // --- 1. alpha.md, then a typed :vsplit into TWO editor panes --------------
    open_alpha(&handle);
    send_ex_command(&handle, ":vsplit");
    // Consume the :vsplit's OWN Split intent so the negative drain below only
    // sees intents from the reuse call.
    let (_, _, vsplit_new_win) = poll_for_split_intent(&handle, "the typed :vsplit");
    wait_for_window_count(&handle, TWO_PANES, "after the typed :vsplit");

    // The active pane is the new split; the OTHER pane is the reuse target.
    let active = wait_for_some_active_win(&handle);
    let other_win = live_windows(&handle)
        .iter()
        .map(|(_, win)| *win)
        .find(|win| *win != active)
        .expect("two live windows must yield a non-active other window");
    assert!(
        active == vsplit_new_win || other_win == vsplit_new_win,
        "the :vsplit's new window must be one of the two live windows"
    );

    // --- 2. preview_in_other(other_win): bind into the target, focus untouched --
    let source_bufnr = handle.preview_in_other(other_win);
    assert!(
        source_bufnr > 0,
        "preview_in_other(other_win) on a markdown pane must return the SOURCE \
         bufnr (> 0), got {source_bufnr}"
    );
    wait_for_previews(&handle, "the other pane previews alpha.md", |p| {
        p.iter()
            .any(|row| row.win() == other_win && row.source_bufnr() == source_bufnr)
    });

    // --- 3. Negatives: no split, no focus move --------------------------------
    assert_no_split_intent(&handle, "reusing an existing pane must NOT split");
    assert_eq!(
        live_window_count(&handle),
        TWO_PANES,
        "reusing an existing pane must keep the window count at two",
    );
    assert_eq!(
        handle.active_win(),
        active,
        "preview_in_other must leave focus in the source-editing pane",
    );
    wait_for_markdown_active(
        &handle,
        true,
        "the focused pane still edits the markdown source",
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn edit_in_other_reuses_an_existing_pane_and_focuses_it() {
    // Case 4 (reuse-edit): from a preview pane with a second pane open, passing
    // that pane's window binds the source there and FOCUSES it; the original
    // preview window keeps its preview.
    //
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let tmp = seed_markdown_cwd();
    let handle = start_attached(&tmp);

    // --- 1. alpha.md, :vsplit, then the in-place preview in the ACTIVE pane ----
    open_alpha(&handle);
    send_ex_command(&handle, ":vsplit");
    let _ = poll_for_split_intent(&handle, "the typed :vsplit");
    wait_for_window_count(&handle, TWO_PANES, "after the typed :vsplit");

    let scratch_bufnr = handle.open_preview();
    assert!(
        scratch_bufnr > 0,
        "open_preview() on a markdown buffer must return a valid preview bufnr \
         (> 0), got {scratch_bufnr}"
    );
    wait_for_preview_active(&handle, true, "the active pane is now the preview");
    let previews = wait_for_previews(&handle, "one alpha.md preview", |p| {
        p.len() == 1 && p.iter().any(|row| row.source_name() == "alpha.md")
    });
    let preview_win = previews[0].win();
    let other_win = live_windows(&handle)
        .iter()
        .map(|(_, win)| *win)
        .find(|win| *win != preview_win)
        .expect("two live windows must yield a non-preview other window");

    // --- 2. edit_in_other(other_win): bind the source there, focus it ---------
    let returned = handle.edit_in_other(other_win);
    assert_ne!(
        returned, 0,
        "edit_in_other(other_win) on the active preview pane must return the \
         bufnr bound into the target (nonzero)"
    );
    wait_for_active_win(
        &handle,
        other_win,
        "edit_in_other moves focus to the pane now EDITING the source",
    );
    wait_for_current_file(&handle, "alpha.md");

    // --- 3. The preview window is untouched; layout unchanged ----------------
    let after = wait_for_previews(&handle, "the preview window keeps its preview", |p| {
        p.iter().any(|row| row.win() == preview_win)
    });
    assert!(
        after.iter().any(|row| row.win() == preview_win),
        "the original preview window must still appear in previews(): {:?}",
        after.iter().map(FfiPreview::win).collect::<Vec<_>>(),
    );
    assert_eq!(
        live_window_count(&handle),
        TWO_PANES,
        "reusing an existing pane must keep the window count at two",
    );
    assert_no_split_intent(&handle, "reusing an existing pane must NOT split");

    handle.shutdown();
    drop(tmp);
}

#[test]
fn gates_reject_without_splitting_and_invalid_target_falls_back_to_split() {
    // Case 5 (gates) + case 6 (invalid target). The gate check runs BEFORE any
    // split, so a rejected call must leave the layout byte-for-byte alone — a
    // mis-timed palette activation is a pure no-op, never a stray split.
    //
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    // alpha.md plus a plain-text file for the non-markdown gate case.
    let tmp = seed_markdown_cwd();
    std::fs::write(tmp.path().join("notes.txt"), "plain text notes\n")
        .expect("failed to write notes.txt");
    let handle = start_attached(&tmp);

    // --- 1. Non-markdown gate: preview_in_other(0) is a 0 no-op, NO split -----
    send_ex_command(&handle, ":e notes.txt");
    wait_for_markdown_active(&handle, false, "after :e notes.txt");
    wait_for_current_file(&handle, "notes.txt");
    let _ = handle.drain_intents();
    assert_eq!(
        handle.preview_in_other(0),
        0,
        "preview_in_other(0) on a non-markdown pane must return the neutral 0"
    );
    assert_no_split_intent(
        &handle,
        "the markdown gate runs BEFORE the split, so a rejected call must not split",
    );
    assert_eq!(
        live_window_count(&handle),
        ONE_PANE,
        "a gate-rejected preview_in_other must leave the single pane alone",
    );
    assert!(
        handle.previews().is_empty(),
        "a gate-rejected preview_in_other must leave previews() empty",
    );

    // --- 2. Off-preview gate: edit_in_other(0) is a 0 no-op -------------------
    open_alpha(&handle);
    wait_for_preview_active(
        &handle,
        false,
        "a plain markdown editor pane is not a preview",
    );
    assert_eq!(
        handle.edit_in_other(0),
        0,
        "edit_in_other(0) off a NON-preview pane must return the neutral 0"
    );
    assert_eq!(
        live_window_count(&handle),
        ONE_PANE,
        "a gate-rejected edit_in_other must leave the single pane alone",
    );

    // --- 3. Invalid target (the active window ITSELF) -> split fallback -------
    let active = wait_for_some_active_win(&handle);
    let _ = handle.drain_intents();
    let source_bufnr = handle.preview_in_other(active);
    assert!(
        source_bufnr > 0,
        "preview_in_other(<the active win>) must fall back to the split path and \
         return the SOURCE bufnr (> 0), got {source_bufnr}"
    );
    let (orientation, split_from, new_win) =
        poll_for_split_intent(&handle, "the invalid-target split fallback");
    assert_eq!(
        orientation, ORIENTATION_VERTICAL,
        "the fallback split must be VERTICAL ({ORIENTATION_VERTICAL})"
    );
    assert_eq!(
        split_from, active,
        "the fallback Split intent's from() must be the active window"
    );
    wait_for_window_count(&handle, TWO_PANES, "the invalid-target fallback splits");
    wait_for_previews(&handle, "the preview landed in the NEW window", |p| {
        p.len() == 1 && p.iter().any(|row| row.win() == new_win)
    });
    assert_ne!(
        new_win, active,
        "the preview must NOT land in the (invalid) target: the active window itself"
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn drawer_target_is_rejected_and_falls_back_to_split() {
    // The terminal drawer is never a bind target: `wolf_other_pane_target_ok`
    // rejects it via `b:wolf_terminal` / `w:wolf_terminal`, so passing the drawer
    // window sends `wolf_preview_in_other` down the split path — the preview
    // lands in a fresh split, and the drawer keeps its terminal buffer.
    //
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let tmp = seed_markdown_cwd();
    let handle = start_attached(&tmp);

    // --- 1. alpha.md active in the single editor pane ------------------------
    open_alpha(&handle);
    let source_win = wait_for_some_active_win(&handle);

    // --- 2. Open the terminal drawer; capture its window from DRAWER_OPEN -----
    handle.toggle_terminal();
    let drawer_win = poll_for_drawer_open_intent(&handle, "toggle_terminal opens the drawer");
    assert_ne!(drawer_win, 0, "the drawer window handle must be nonzero");
    wait_for_window_count(&handle, TWO_PANES, "the drawer window is open");

    // --- 3. Focus back to the markdown pane ----------------------------------
    // The open drawer is focused in terminal-insert mode: `<C-\><C-n>` leaves it
    // (the `terminal.rs` idiom), then `:wincmd p` returns to the previous window
    // — the markdown pane the drawer was opened from.
    handle.send_key(ctrl_char_key('\\'));
    handle.send_key(ctrl_char_key('n'));
    send_ex_command(&handle, ":wincmd p");
    wait_for_active_win(&handle, source_win, "back in the markdown pane");
    wait_for_markdown_active(&handle, true, "the markdown pane is focused again");
    let _ = handle.drain_intents();

    // --- 4. preview_in_other(<drawer win>): rejected target -> split fallback --
    let source_bufnr = handle.preview_in_other(drawer_win);
    assert!(
        source_bufnr > 0,
        "preview_in_other(<the drawer win>) must fall back to the split path and \
         return the SOURCE bufnr (> 0), got {source_bufnr}"
    );
    let (orientation, split_from, new_win) =
        poll_for_split_intent(&handle, "the drawer-target split fallback");
    assert_eq!(
        orientation, ORIENTATION_VERTICAL,
        "the fallback split must be VERTICAL ({ORIENTATION_VERTICAL})"
    );
    assert_eq!(
        split_from, source_win,
        "the fallback Split intent's from() must be the markdown pane"
    );
    assert_ne!(
        new_win, drawer_win,
        "the split's new window must be DISTINCT from the drawer window"
    );
    wait_for_window_count(
        &handle,
        THREE_WINDOWS,
        "the drawer-target fallback splits (source + drawer + new split)",
    );

    // --- 5. The preview landed in the NEW split, never in the drawer ----------
    let previews = wait_for_previews(&handle, "the preview landed in the new split", |p| {
        p.len() == 1 && p.iter().any(|row| row.win() == new_win)
    });
    assert!(
        !previews.iter().any(|row| row.win() == drawer_win),
        "no preview row may ever bind the drawer window: {:?}",
        previews.iter().map(FfiPreview::win).collect::<Vec<_>>(),
    );

    // The drawer's buffer is untouched: its single term:// buffer still exists.
    let terms = term_bufnrs(&handle);
    assert_eq!(
        terms.len(),
        1,
        "the drawer must keep its single terminal buffer (got {terms:?})"
    );

    // Focus rule unchanged: the source editor stays focused after the fallback.
    wait_for_active_win(
        &handle,
        source_win,
        "preview_in_other keeps focus in the pane editing the source",
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn edit_in_other_falls_back_to_edit_when_the_source_bufnr_is_stale() {
    // The stale-bufnr fallback in `wolf_edit_in_other`'s `bind_source_into`: when
    // `b:wolf_preview_source_bufnr` no longer names a valid buffer (the source was
    // wiped), the helper focuses the bound window and `:edit`s the source PATH
    // instead — and still returns `nvim_win_get_buf` of the bound window, so the
    // fallback reports non-zero (the deliberate divergence from
    // `wolf_edit_source`'s `source_bufnr or 0`).
    //
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let tmp = seed_markdown_cwd();
    let handle = start_attached(&tmp);

    // --- 1. alpha.md, then the in-place preview (active pane becomes preview) --
    open_alpha(&handle);
    let scratch_bufnr = handle.open_preview();
    assert!(
        scratch_bufnr > 0,
        "open_preview() on a markdown buffer must return a valid preview bufnr \
         (> 0), got {scratch_bufnr}"
    );
    wait_for_preview_active(&handle, true, "the active pane is now the preview");
    let previews = wait_for_previews(&handle, "one alpha.md preview", |p| {
        p.len() == 1 && p.iter().any(|row| row.source_name() == "alpha.md")
    });
    let preview_win = previews[0].win();
    let stale_source_bufnr = previews[0].source_bufnr();
    assert!(
        stale_source_bufnr > 0,
        "the preview row must carry alpha.md's real source bufnr (> 0), got \
         {stale_source_bufnr}"
    );

    // --- 2. Wipe the source buffer so the recorded bufnr goes stale ------------
    // The active pane shows the (normal-mode) scratch, so a typed `:bwipeout N`
    // wipes the undisplayed, unmodified source buffer. The picker losing its
    // alpha.md row is the deterministic wipe barrier.
    send_ex_command(&handle, &format!(":bwipeout {stale_source_bufnr}"));
    wait_for_ranked(&handle, "the wiped source leaves the picker", |rows| {
        rows_ending_with(rows, "alpha.md") == 0
    });
    let _ = handle.drain_intents();

    // --- 3. edit_in_other(0): the stale bufnr takes the :edit fallback ---------
    let returned = handle.edit_in_other(0);
    assert_ne!(
        returned, 0,
        "edit_in_other(0) with a stale source bufnr must still return the bufnr \
         bound into the target (the :edit fallback reports non-zero)"
    );
    assert_ne!(
        returned, stale_source_bufnr,
        "the :edit fallback must produce a FRESH buffer for the source path \
         (bufnrs are never reused within a session)"
    );
    let (orientation, split_from, new_win) =
        poll_for_split_intent(&handle, "the stale-bufnr edit_in_other split");
    assert_eq!(
        orientation, ORIENTATION_VERTICAL,
        "the edit_in_other split must be VERTICAL ({ORIENTATION_VERTICAL})"
    );
    assert_eq!(
        split_from, preview_win,
        "the Split intent's from() must be the (formerly preview) original window"
    );
    wait_for_window_count(&handle, TWO_PANES, "edit_in_other(0) splits");

    // --- 4. The source is edited again in the focused original window ---------
    wait_for_active_win(
        &handle,
        preview_win,
        "edit_in_other moves focus to the pane now EDITING the source",
    );
    wait_for_current_file(&handle, "alpha.md");
    wait_for_preview_active(
        &handle,
        false,
        "the focused pane edits the re-opened source, so is_preview_active falls",
    );
    wait_for_markdown_active(
        &handle,
        true,
        "the focused pane shows the markdown source again",
    );
    // The scratch stays in the NEW (right) window.
    wait_for_previews(&handle, "the scratch moved to the new window", |p| {
        p.iter().any(|row| row.win() == new_win)
    });

    handle.shutdown();
    drop(tmp);
}
