//! Integration tests for the fs-event-driven RESCAN of a live `FileSearch`
//! session (the sidebar-staleness bug fix): after a write of a NEW file, a
//! rename, or a delete performed through the captive Neovim, a long-lived
//! session started via `WolfCore::start_file_search` must re-walk and swap its
//! entry list (bumping `version` and firing the session wake) so the tree /
//! picker reflect the filesystem.
//!
//! Modeled on `file_search.rs` (same REAL, hermetic nvim via
//! `tests/common/mod.rs`, same wake-channel + bounded-poll structure). The
//! trigger path under test is: nvim autocmd -> `wolf_fs_changed` notify ->
//! `RedrawHandler` -> `SearchRegistry` debounced rescan -> walk-then-swap on
//! the session state. Two trigger classes feed that path from
//! `runtime/nvim/init.lua`: buffer events (`BufWritePost` conditional;
//! `BufFilePost` / `BufDelete` force) carrying the touched path, and pathless
//! FORCE events (`ShellCmdPost` / `FocusGained` / `TermLeave`) that catch
//! filesystem changes no buffer event names — shell commands, external tools,
//! the terminal drawer.

mod common;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use wolf_core::files::FileSearchState;
use wolf_core::{FileEntry, FileSearch, InputEvent, Key, WakeFn, WolfCore};

/// Upper bound for a single file search to reach `complete`. Mirrors
/// `file_search.rs::SEARCH_TIMEOUT`.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Upper bound for a triggered rescan to land: covers the registry debounce
/// (default 200 ms via `WOLF_RESCAN_DEBOUNCE_MS`) plus a full re-walk of a tiny
/// temp tree plus nvim's autocmd -> notify latency.
const RESCAN_TIMEOUT: Duration = Duration::from_secs(10);

/// How long to block on a wake before re-checking the predicate (so we make
/// progress even if a wake is missed/coalesced). Mirrors `file_search.rs`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Settle window for the NEGATIVE assertion (":w of an already-listed file does
/// not bump version"). Must sit WELL past the 200 ms rescan debounce plus a
/// worst-case tiny-tree walk, so a wrongly-triggered rescan would have landed
/// before we sample. The tradeoff is inherent to a negative assertion: we can
/// only prove "no rescan within this window", not "never", so the window is
/// chosen ~7x the debounce to make a false pass unlikely without making the
/// test slow.
const DEBOUNCE_SETTLE: Duration = Duration::from_millis(1500);

// --- Helpers ---------------------------------------------------------------

/// Creates a fresh temp dir to serve as a core `cwd` and returns both the guard
/// (kept alive by the caller) and the CANONICAL path to it. Mirrors
/// `file_search.rs::temp_cwd` (canonical so the traversal root, the autocmd's
/// `:p`-expanded paths, and the relative `display` strings all line up).
fn temp_cwd() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let cwd = fs::canonicalize(dir.path()).expect("failed to canonicalize temp cwd");
    (dir, cwd)
}

/// Writes `contents` to `path`, creating parent directories as needed.
fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("failed to create parent dir");
    }
    fs::write(path, contents).expect("failed to write file");
}

/// Builds a `WakeFn` that signals on a channel, paired with the receiver the
/// poll loops wait on. Mirrors `file_search.rs::search_wake`.
fn search_wake() -> (WakeFn, Receiver<()>) {
    let (tx, rx): (Sender<()>, Receiver<()>) = mpsc::channel();
    let wake: WakeFn = Arc::new(move || {
        // A send error just means the receiver was dropped (test finished).
        let _ = tx.send(());
    });
    (wake, rx)
}

/// Starts a `WolfCore` rooted at `cwd` (default grid size), waits for the
/// initial attach/flush, and returns the core plus the retained `FlushWaiter`
/// (every test here drives Ex commands, so the waiter is always needed).
/// Mirrors `file_search.rs::start_core_in_with_waiter`.
fn start_core_in(cwd: &Path) -> (WolfCore, common::FlushWaiter) {
    let (wake, waiter) = common::wake_channel();
    let core = WolfCore::start(
        common::config_in(
            cwd.to_path_buf(),
            common::DEFAULT_COLS,
            common::DEFAULT_ROWS,
        ),
        wake,
    )
    .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    // The attach flush (seq 0 -> 1) already landed during WolfCore::start, so use
    // the absolute-threshold initial-flush wait (an advance-based wait would hang).
    waiter.wait_for_initial_flush(&core, common::FLUSH_TIMEOUT);
    (core, waiter)
}

/// Types `cmd` as an Ex command (WITHOUT the leading `:`, which this adds) and
/// presses Enter, then waits for nvim to process it (a redraw flush). Mirrors
/// the `:cd` key-driving pattern in `file_search.rs`.
fn send_ex(core: &WolfCore, waiter: &common::FlushWaiter, cmd: &str) {
    core.send_input(common::ch(':'));
    common::type_str(core, cmd);
    core.send_input(common::key(Key::Enter));
    waiter.wait_for_flush(core, common::FLUSH_TIMEOUT);
}

/// Enters insert mode, types `text`, and leaves insert mode, waiting for the
/// resulting redraw flush so the edit has landed before the caller proceeds.
fn insert_text(core: &WolfCore, waiter: &common::FlushWaiter, text: &str) {
    core.send_input(common::ch('i'));
    common::type_str(core, text);
    core.send_input(common::key(Key::Esc));
    waiter.wait_for_flush(core, common::FLUSH_TIMEOUT);
}

/// Polls `search` until its view reports `complete`, waking on `wakes`, then
/// returns a snapshot of the collected entries. Panics on timeout. Mirrors
/// `file_search.rs::poll_until_complete`.
fn poll_until_complete(search: &FileSearch, wakes: &Receiver<()>) -> Vec<FileEntry> {
    let deadline = Instant::now() + SEARCH_TIMEOUT;
    loop {
        {
            let view = search.read();
            if view.complete {
                return view.entries.to_vec();
            }
        }
        if Instant::now() >= deadline {
            panic!("file search did not reach `complete` within {SEARCH_TIMEOUT:?}");
        }
        // Bounded wait: a wake nudges us promptly; the timeout guarantees we
        // re-check even if a wake was coalesced/missed.
        let _ = wakes.recv_timeout(POLL_INTERVAL);
    }
}

/// Polls `search` until `pred` accepts its state (the rescan-landed check),
/// waking on `wakes`, then returns a snapshot of the entries. Panics after
/// [`RESCAN_TIMEOUT`] with `desc` and the entries seen last.
fn poll_search_until(
    search: &FileSearch,
    wakes: &Receiver<()>,
    desc: &str,
    mut pred: impl FnMut(&FileSearchState) -> bool,
) -> Vec<FileEntry> {
    let deadline = Instant::now() + RESCAN_TIMEOUT;
    loop {
        {
            let view = search.read();
            if pred(&view) {
                return view.entries.to_vec();
            }
        }
        if Instant::now() >= deadline {
            let names: Vec<String> = search
                .read()
                .entries
                .iter()
                .map(|e| e.display.clone())
                .collect();
            panic!("timed out after {RESCAN_TIMEOUT:?} waiting for: {desc}; entries={names:?}");
        }
        let _ = wakes.recv_timeout(POLL_INTERVAL);
    }
}

/// Polls `core.mru_recent()` until it contains `path`, or panics after
/// [`SEARCH_TIMEOUT`]. Mirrors `mru_open_records.rs::wait_for_mru_contains`:
/// the record happens in the input pump and may trail the open's redraw flush,
/// so a poll (not a flush wait) is the reliable ordering point.
fn wait_for_mru_contains(core: &WolfCore, path: &Path) {
    let deadline = Instant::now() + SEARCH_TIMEOUT;
    loop {
        let recent = core.mru_recent();
        if recent.iter().any(|e| paths_equal(&e.path, path)) {
            return;
        }
        if Instant::now() >= deadline {
            let listed: Vec<_> = recent.iter().map(|e| e.path.clone()).collect();
            panic!(
                "timed out after {SEARCH_TIMEOUT:?} waiting for the MRU to contain {}; \
                 mru_recent() = {listed:?}",
                path.display()
            );
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Compares two paths by their canonical form when resolvable, else raw
/// equality. Mirrors `mru_open_records.rs::paths_equal`.
fn paths_equal(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

/// The set of `display` strings of `entries` (traversal order is
/// nondeterministic, so tests assert on the SET rather than a sequence).
fn displays(entries: &[FileEntry]) -> HashSet<String> {
    entries.iter().map(|e| e.display.clone()).collect()
}

/// How many entries carry `display == name` — the dedup assertions key on this
/// being exactly 1 (a rescan must never double-list a file).
fn count_of(entries: &[FileEntry], name: &str) -> usize {
    entries.iter().filter(|e| e.display == name).count()
}

/// Blocks (bounded by [`SEARCH_TIMEOUT`]) until `pred` holds, sleeping
/// [`POLL_INTERVAL`] between checks. Settling helper for async effects that
/// have no wake channel of their own (the drawer window opening, a mode
/// change landing in the snapshot). Panics with `desc` on timeout.
fn wait_until(desc: &str, mut pred: impl FnMut() -> bool) {
    let deadline = Instant::now() + SEARCH_TIMEOUT;
    while !pred() {
        if Instant::now() >= deadline {
            panic!("timed out after {SEARCH_TIMEOUT:?} waiting for: {desc}");
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// The active cursor mode's short name (e.g. `"normal"`, `"terminal"`) from
/// the snapshot's cached `mode_info_set`. Returned as an OWNED `String` so the
/// snapshot read-guard drops before the caller's next poll sleep — a guard
/// held across a sleep starves the redraw handler's `try_write` (see the lock
/// discipline notes in `autoread.rs`).
fn current_mode_name(core: &WolfCore) -> String {
    let snap = core.read_snapshot();
    snap.mode_infos
        .get(snap.cursor.mode_idx)
        .map(|m| m.name.clone())
        .unwrap_or_default()
}

/// Whether any listed buffer is a terminal buffer, detected by Neovim's
/// `term://` naming convention (terminal buffers are buflisted — mirrors the
/// detection in `wolf-ffi/tests/terminal.rs`).
fn has_terminal_buffer(core: &WolfCore) -> bool {
    core.list_buffers()
        .iter()
        .any(|b| b.name.starts_with("term://"))
}

/// Forces a genuine unfocused→focused transition on the captive nvim UI, which
/// fires `FocusGained`. Sends `false` FIRST because nvim de-dupes
/// `FocusGained` (it fires only on the unfocused→focused EDGE) and a
/// freshly-attached UI's initial focus state is uncertain, so a lone `true`
/// could no-op. Mirrors `autoread.rs::refocus_ui` and the production
/// resign→activate sequence.
fn refocus_ui(core: &WolfCore) {
    core.set_ui_focus(false);
    core.set_ui_focus(true);
}

// --- Tests -----------------------------------------------------------------

/// (1) A NEW file created through nvim (`:e fresh.md`, insert, `:w`) appears in
/// a live, already-`complete` session: the rescan swaps the list, bumps
/// `version`, keeps `complete = true`, and does not duplicate any entry.
#[test]
fn new_file_appears_after_write() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    write_file(&cwd.join("existing.md"), "e");

    let (core, waiter) = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = core.start_file_search(wake);
    let baseline = poll_until_complete(&search, &wakes);
    assert!(
        displays(&baseline).contains("existing.md"),
        "sanity: the seeded file should be in the baseline; got {:?}",
        displays(&baseline)
    );
    let v0 = search.read().version;

    // Create fresh.md entirely through the captive nvim: the `:w` fires
    // BufWritePost -> wolf_fs_changed -> debounced rescan.
    send_ex(&core, &waiter, "e fresh.md");
    insert_text(&core, &waiter, "hello");
    send_ex(&core, &waiter, "w");

    let entries = poll_search_until(&search, &wakes, "fresh.md to appear after :w", |view| {
        view.entries.iter().any(|e| e.display == "fresh.md")
    });

    {
        let view = search.read();
        assert!(
            view.version > v0,
            "the rescan swap must bump version past the pre-write value \
             (was {v0}, now {})",
            view.version
        );
        assert!(
            view.complete,
            "the session must stay `complete` across a rescan swap"
        );
    }
    assert_eq!(
        count_of(&entries, "fresh.md"),
        1,
        "the new file must be listed exactly once; got {:?}",
        displays(&entries)
    );
    assert_eq!(
        count_of(&entries, "existing.md"),
        1,
        "a rescan must not duplicate an already-listed file; got {:?}",
        displays(&entries)
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (2) A file deleted from disk and `:bdelete!`d disappears from a live
/// session. The file is opened first via `InputEvent::Open` — the input pump's
/// MRU-recording path — so it verifiably lands in the MRU; the rescan re-seeds
/// from `MruStore::recent(&root)`, which prunes rows for deleted files, so the
/// MRU seed must NOT resurrect the entry.
#[test]
fn deleted_file_disappears() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    write_file(&cwd.join("doomed.md"), "d");
    write_file(&cwd.join("keeper.md"), "k");

    let (core, waiter) = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = core.start_file_search(wake);
    let baseline = poll_until_complete(&search, &wakes);
    assert!(
        displays(&baseline).contains("doomed.md"),
        "sanity: doomed.md should be listed before the delete; got {:?}",
        displays(&baseline)
    );

    // Open via `InputEvent::Open` and wait for the MRU record, or the resurrection guard is vacuous;
    // then delete the file and `:bdelete!`, whose BufDelete forces a rescan.
    let doomed = cwd.join("doomed.md");
    core.send_input(InputEvent::Open {
        path: doomed.clone(),
    });
    waiter.wait_for_flush(&core, common::FLUSH_TIMEOUT);
    wait_for_mru_contains(&core, &doomed);
    fs::remove_file(&doomed).expect("failed to remove doomed.md");
    send_ex(&core, &waiter, "bdelete!");

    let entries = poll_search_until(
        &search,
        &wakes,
        "doomed.md to disappear after remove_file + :bdelete!",
        |view| !view.entries.iter().any(|e| e.display == "doomed.md"),
    );

    assert!(
        displays(&entries).contains("keeper.md"),
        "the surviving file must still be listed; got {:?}",
        displays(&entries)
    );
    assert!(
        search.read().complete,
        "the session must stay `complete` across a delete-triggered rescan"
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (3) `:saveas renamed.md` lists the NEW name — and the OLD name stays listed,
/// because `:saveas` genuinely leaves the original file on disk (it writes a
/// copy under the new name and re-binds the buffer; it is not an `mv`). The
/// rescan must reflect the filesystem truthfully: both files exist, both are
/// listed. Pinning this semantic here keeps a later "fix" from wrongly dropping
/// the old name.
#[test]
fn saveas_lists_new_name_and_keeps_old() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    write_file(&cwd.join("orig.md"), "o");

    let (core, waiter) = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = core.start_file_search(wake);
    let baseline = poll_until_complete(&search, &wakes);
    assert!(
        displays(&baseline).contains("orig.md"),
        "sanity: orig.md should be listed before the saveas; got {:?}",
        displays(&baseline)
    );

    send_ex(&core, &waiter, "e orig.md");
    send_ex(&core, &waiter, "saveas renamed.md");

    let entries = poll_search_until(
        &search,
        &wakes,
        "renamed.md to appear after :saveas",
        |view| view.entries.iter().any(|e| e.display == "renamed.md"),
    );

    // Ground truth: both files exist on disk after a :saveas.
    assert!(
        cwd.join("orig.md").exists(),
        ":saveas must leave the original file on disk"
    );
    assert!(
        displays(&entries).contains("orig.md"),
        "the old name genuinely remains on disk after :saveas, so it must stay \
         listed; got {:?}",
        displays(&entries)
    );
    assert_eq!(
        count_of(&entries, "renamed.md"),
        1,
        "the new name must be listed exactly once; got {:?}",
        displays(&entries)
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (4) `:w` of an EXISTING, already-listed file triggers no rescan: the
/// `BufWritePost` path is conditional (rescan only when the written path is not
/// already in the session's `seen` set), so the version must not move.
///
/// NEGATIVE-ASSERTION TIMING: we cannot wait for an event that must not happen,
/// so we sleep [`DEBOUNCE_SETTLE`] (~7x the 200 ms rescan debounce) and then
/// assert the version is unchanged. A wrongly-triggered rescan of this tiny
/// tree would land well inside that window; the residual risk of a false pass
/// on a pathologically slow host is the accepted tradeoff.
#[test]
fn write_of_existing_file_does_not_bump_version() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    write_file(&cwd.join("stable.md"), "s");

    let (core, waiter) = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = core.start_file_search(wake);
    let baseline = poll_until_complete(&search, &wakes);
    assert!(
        displays(&baseline).contains("stable.md"),
        "sanity: stable.md should be listed; got {:?}",
        displays(&baseline)
    );

    // Open the listed file FIRST, then capture the version: `:e` itself must not
    // move it (only rescans do), and capturing after the open isolates the `:w`.
    send_ex(&core, &waiter, "e stable.md");
    let v0 = search.read().version;

    // `:w` always writes (even an unmodified buffer), so BufWritePost fires —
    // but the path is already in `seen`, so no rescan may follow.
    send_ex(&core, &waiter, "w");

    std::thread::sleep(DEBOUNCE_SETTLE);
    let v1 = search.read().version;
    assert_eq!(
        v0, v1,
        ":w of an already-listed file must not trigger a rescan / version bump \
         (was {v0}, now {v1})"
    );
    assert!(
        search.read().complete,
        "the session must remain `complete` (nothing should have changed)"
    );

    drop(wakes);
    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (5) A BURST of three new files created in quick succession all appear, each
/// exactly once. Deliberately NO assertion on how many rescans ran (the
/// debounce may coalesce them into one walk or run several — either is
/// correct, and counting would be flaky); only the converged result matters.
#[test]
fn burst_of_new_files_all_appear_deduped() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    write_file(&cwd.join("base.md"), "b");

    let (core, waiter) = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = core.start_file_search(wake);
    poll_until_complete(&search, &wakes);

    // Three back-to-back `:e` + `:w` pairs inside one another's debounce windows; writing a new
    // empty buffer creates the file and fires BufWritePost, so no insert is needed.
    let names = ["burst_a.md", "burst_b.md", "burst_c.md"];
    for name in names {
        send_ex(&core, &waiter, &format!("e {name}"));
        send_ex(&core, &waiter, "w");
    }

    let entries = poll_search_until(
        &search,
        &wakes,
        "all three burst files to appear after the writes",
        |view| {
            names
                .iter()
                .all(|name| view.entries.iter().any(|e| e.display == *name))
        },
    );

    for name in names {
        assert_eq!(
            count_of(&entries, name),
            1,
            "{name} must be listed exactly once after the burst; got {:?}",
            displays(&entries)
        );
    }
    assert_eq!(
        count_of(&entries, "base.md"),
        1,
        "the pre-existing file must not be duplicated by the rescans; got {:?}",
        displays(&entries)
    );
    assert!(
        search.read().complete,
        "the session must stay `complete` after the burst settles"
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (6) A file removed by a SHELL command run through nvim disappears from a
/// live session: `ShellCmdPost` is a pathless FORCE trigger, so the rescan
/// fires even though no buffer event names the deleted file. `:silent !rm`
/// (not a bare `:!rm`) because `silent` suppresses the hit-enter prompt that
/// would wedge the key-driven harness — `ShellCmdPost` still fires for a
/// `:silent !…`. This exercises both halves at once: the trigger, and the
/// shell genuinely performing the deletion on disk.
#[test]
fn shell_cmd_post_rescans() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    write_file(&cwd.join("doomed.md"), "d");
    write_file(&cwd.join("keeper.md"), "k");

    let (core, waiter) = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = core.start_file_search(wake);
    let baseline = poll_until_complete(&search, &wakes);
    assert!(
        displays(&baseline).contains("doomed.md"),
        "sanity: doomed.md should be listed before the shell delete; got {:?}",
        displays(&baseline)
    );

    // The absolute path keeps the command independent of nvim's cwd; a temp-dir
    // path carries no spaces or shell metacharacters, so no quoting is needed.
    let doomed = cwd.join("doomed.md");
    send_ex(&core, &waiter, &format!("silent !rm {}", doomed.display()));

    let entries = poll_search_until(
        &search,
        &wakes,
        "doomed.md to disappear after :silent !rm",
        |view| !view.entries.iter().any(|e| e.display == "doomed.md"),
    );

    // Ground truth: the shell really deleted the file (the rescan reflected the
    // filesystem, it did not invent the removal).
    assert!(
        !doomed.exists(),
        ":silent !rm must have removed the file from disk"
    );
    assert!(
        displays(&entries).contains("keeper.md"),
        "the surviving file must still be listed; got {:?}",
        displays(&entries)
    );
    assert!(
        search.read().complete,
        "the session must stay `complete` across a ShellCmdPost-triggered rescan"
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (7) A file removed EXTERNALLY (plain `std::fs::remove_file`, no nvim
/// involvement at all) disappears when the UI regains focus: `FocusGained` is
/// a pathless FORCE trigger, catching changes made in an external terminal /
/// Finder while the app was in the background. The trigger is driven through
/// `set_ui_focus`, the same production path the front-end uses on OS
/// activation (and the same edge `WolfAutoRead`'s `:checktime` rides).
#[test]
fn focus_gained_rescans() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    write_file(&cwd.join("ext.md"), "e");
    write_file(&cwd.join("keeper.md"), "k");

    let (core, _waiter) = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = core.start_file_search(wake);
    let baseline = poll_until_complete(&search, &wakes);
    assert!(
        displays(&baseline).contains("ext.md"),
        "sanity: ext.md should be listed before the external delete; got {:?}",
        displays(&baseline)
    );

    // Delete outside nvim, then push both focus edges: nvim de-dupes FocusGained, so only a real
    // unfocused-to-focused transition fires it (see `refocus_ui`).
    fs::remove_file(cwd.join("ext.md")).expect("failed to remove ext.md");
    refocus_ui(&core);

    let entries = poll_search_until(
        &search,
        &wakes,
        "ext.md to disappear after the refocus edge",
        |view| !view.entries.iter().any(|e| e.display == "ext.md"),
    );

    assert!(
        displays(&entries).contains("keeper.md"),
        "the surviving file must still be listed; got {:?}",
        displays(&entries)
    );
    assert!(
        search.read().complete,
        "the session must stay `complete` across a FocusGained-triggered rescan"
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (8) A file removed while the terminal DRAWER is open disappears once the
/// drawer is toggled closed. The REAL drawer flow is driven — `toggle_terminal`
/// twice, never a `doautocmd` — because this test asserts the FLOW (drawer
/// close ⇒ rescan), not the event name: the drawer auto-`startinsert`s on
/// WinEnter (the `WolfTerminalInsert` augroup), so the open toggle puts nvim in
/// Terminal-mode and the close toggle exits it (t → n), firing `TermLeave`, a
/// pathless FORCE trigger.
///
/// Contingency (spec): if `TermLeave` proves not to fire on the drawer close,
/// the fix is a `WinLeave` autocmd guarded on `buftype == "terminal"` in
/// `init.lua` — and this test stays as-is.
#[test]
fn term_leave_rescans() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    write_file(&cwd.join("gone.md"), "g");
    write_file(&cwd.join("keeper.md"), "k");

    let (core, _waiter) = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = core.start_file_search(wake);
    let baseline = poll_until_complete(&search, &wakes);
    assert!(
        displays(&baseline).contains("gone.md"),
        "sanity: gone.md should be listed before the drawer delete; got {:?}",
        displays(&baseline)
    );

    // Let the drawer settle (term:// buffer listed, Terminal mode reported): only a settled t-mode
    // drawer makes the close toggle produce the t -> n TermLeave edge.
    core.toggle_terminal();
    wait_until("the drawer's term:// buffer to be listed", || {
        has_terminal_buffer(&core)
    });
    wait_until("nvim to enter Terminal-mode in the drawer", || {
        current_mode_name(&core) == "terminal"
    });

    // Delete from outside nvim while the drawer is open, standing in for a command run in its
    // shell, which is too slow and nondeterministic to drive; then close the drawer.
    fs::remove_file(cwd.join("gone.md")).expect("failed to remove gone.md");
    core.toggle_terminal();

    let entries = poll_search_until(
        &search,
        &wakes,
        "gone.md to disappear after the drawer close",
        |view| !view.entries.iter().any(|e| e.display == "gone.md"),
    );

    assert!(
        displays(&entries).contains("keeper.md"),
        "the surviving file must still be listed; got {:?}",
        displays(&entries)
    );
    assert!(
        search.read().complete,
        "the session must stay `complete` across a TermLeave-triggered rescan"
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}
