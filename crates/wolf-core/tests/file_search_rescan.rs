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
//!
//! The file has two halves. Cases (1) to (8) cover those nvim triggers and run
//! with core's filesystem watcher OFF (`WOLF_FS_WATCH=0`): with it on, the
//! watcher would surface the same change and a deleted trigger would fail no
//! test. Cases (a) to (f) cover the watcher: they touch the tree only through
//! `std::fs`, with no Ex command and no `set_ui_focus`, so nothing but the
//! watcher can surface the change.
//!
//! `WOLF_FS_WATCH` is process-global and read once per search, on the calling
//! thread of `start_file_search`. Every case therefore starts its session
//! through [`start_search`], which sets or clears the variable under
//! [`WATCH_ENV_LOCK`]; a direct `start_file_search` could read a transient `0`
//! a parallel case set.

mod common;

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
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

/// Rounds of churn under the ignored tree in
/// `ignored_churn_leaves_version_unchanged`.
const CHURN_ROUNDS: usize = 60;

/// Serializes the `{set or clear WOLF_FS_WATCH; start_file_search; clear}`
/// critical section in [`start_search`]. Mirrors `file_search_bounds.rs`'s
/// `BOUNDS_ENV_LOCK`.
static WATCH_ENV_LOCK: Mutex<()> = Mutex::new(());

// --- Helpers ---------------------------------------------------------------

/// Acquires [`WATCH_ENV_LOCK`], recovering from poisoning so a panic in one
/// case does not cascade into every later one. Mirrors
/// `file_search_bounds.rs::lock_env`.
fn lock_env() -> MutexGuard<'static, ()> {
    WATCH_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Starts a file search on `core` with the filesystem watcher on (`watched`)
/// or off (`WOLF_FS_WATCH=0`). The only way any case in this binary starts a
/// session; see the module doc for why.
fn start_search(core: &WolfCore, wake: WakeFn, watched: bool) -> FileSearch {
    let _env = lock_env();
    if watched {
        std::env::remove_var("WOLF_FS_WATCH");
    } else {
        std::env::set_var("WOLF_FS_WATCH", "0");
    }
    let search = core.start_file_search(wake);
    std::env::remove_var("WOLF_FS_WATCH");
    search
}

/// Sleeps [`DEBOUNCE_SETTLE`]. Every watcher case calls it right after
/// `poll_until_complete`, so the catch-up rescan a new watch forwards and any
/// startup trigger have drained before the case touches the tree. Without it,
/// a late catch-up walk could surface the change with no live watch at all,
/// and the falsifier "skip the watch call" would pass (a) and (b).
fn settle() {
    std::thread::sleep(DEBOUNCE_SETTLE);
}

/// Seeds the tree shared by the two negative watcher cases, (c) and (d):
/// `build/` and `*.log` ignored by `.gitignore`, a listed `keep.md`, and
/// `primed.md` hidden only by `.git/info/exclude` (which [`prime`] empties).
fn seed_primed_tree(cwd: &Path) {
    write_file(&cwd.join(".gitignore"), "build/\n*.log\n");
    write_file(&cwd.join("build/seed.o"), "o");
    write_file(&cwd.join("keep.md"), "k");
    write_file(&cwd.join("primed.md"), "p");
    write_file(&cwd.join(".git/info/exclude"), "primed.md\n");
}

/// Proves the watcher is live before a negative assertion relies on its
/// silence: writes `sentinel.md` with `std::fs`, polls until a watcher-driven
/// rescan lists it, then [`settle`]s. Without this, a watcher that never
/// started would pass (c) and (d) vacuously.
fn prove_watcher_live(cwd: &Path, search: &FileSearch, wakes: &Receiver<()>) {
    write_file(&cwd.join("sentinel.md"), "s");
    poll_search_until(
        search,
        wakes,
        "sentinel.md to appear through the watcher (is the watch live?)",
        |view| view.entries.iter().any(|e| e.display == "sentinel.md"),
    );
    settle();
}

/// Makes the next re-walk's answer differ from the live listing, then returns
/// the session's `version`. A no-op rescan bumps nothing (the equality skip in
/// the swap), so without this a wrongly-forwarded event would go unseen.
///
/// The difference is made by emptying `.git/info/exclude`, so `primed.md`
/// becomes listable. The watcher drops that edit by a rule other than the ones
/// (c) and (d) test (the `.git` component), and a rescan that does run, for
/// any reason, lists `primed.md` and bumps `version`.
fn prime(cwd: &Path, search: &FileSearch) -> u64 {
    let before = search.read().version;
    fs::write(cwd.join(".git/info/exclude"), "").expect("failed to empty .git/info/exclude");
    settle();
    let view = search.read();
    assert_eq!(
        view.version, before,
        "precondition: an edit under `.git` must reach no rescan (the watcher drops `.git` \
         events); a bump means `.git` paths are forwarded (was {before}, now {})",
        view.version
    );
    assert!(
        !view.entries.iter().any(|e| e.display == "primed.md"),
        "precondition: primed.md must stay unlisted until a re-walk runs; listed means a \
         rescan ran on the `.git/info/exclude` edit; got {:?}",
        displays(&view.entries)
    );
    view.version
}

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

/// Appends `contents` to `path` in place (creating it if absent), so the
/// watcher sees a modify of the same inode rather than a replace.
fn append_to(path: &Path, contents: &str) {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("failed to open file for append");
    file.write_all(contents.as_bytes())
        .expect("failed to append to file");
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
/// `version`, keeps `complete = true`, and does not duplicate any entry. The
/// watcher is off, so only the `:w`'s `BufWritePost` can surface the file.
#[test]
fn new_file_appears_after_write() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    write_file(&cwd.join("existing.md"), "e");

    let (core, waiter) = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = start_search(&core, wake, false);
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
/// MRU seed must NOT resurrect the entry. The watcher is off, so only the
/// `BufDelete` can surface the delete.
#[test]
fn deleted_file_disappears() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    write_file(&cwd.join("doomed.md"), "d");
    write_file(&cwd.join("keeper.md"), "k");

    let (core, waiter) = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = start_search(&core, wake, false);
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
/// the old name. The watcher is off, so only nvim's buffer events can surface
/// the new name.
#[test]
fn saveas_lists_new_name_and_keeps_old() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    write_file(&cwd.join("orig.md"), "o");

    let (core, waiter) = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = start_search(&core, wake, false);
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
/// already in the session's `seen` set), so the version must not move. The
/// watcher is off, so the `:w` is the only event the registry sees.
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
    let search = start_search(&core, wake, false);
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
/// The watcher is off, so only the `:w`s' `BufWritePost`s can surface them.
#[test]
fn burst_of_new_files_all_appear_deduped() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    write_file(&cwd.join("base.md"), "b");

    let (core, waiter) = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = start_search(&core, wake, false);
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
/// shell genuinely performing the deletion on disk. The watcher is off, so
/// only `ShellCmdPost` can surface the delete.
#[test]
fn shell_cmd_post_rescans() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    write_file(&cwd.join("doomed.md"), "d");
    write_file(&cwd.join("keeper.md"), "k");

    let (core, waiter) = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = start_search(&core, wake, false);
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
/// activation (and the same edge `WolfAutoRead`'s `:checktime` rides). The
/// watcher is off, so only `FocusGained` can surface the delete.
#[test]
fn focus_gained_rescans() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    write_file(&cwd.join("ext.md"), "e");
    write_file(&cwd.join("keeper.md"), "k");

    let (core, _waiter) = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = start_search(&core, wake, false);
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
/// pathless FORCE trigger. The watcher is off, so only `TermLeave` can surface
/// the delete.
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
    let search = start_search(&core, wake, false);
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

// --- Watcher cases (WOLF_FS_WATCH unset) -------------------------------------

/// (a) A file created with `std::fs` alone, with no Neovim event and no focus
/// edge, appears in a live session: the watcher's create event reaches the
/// registry, the path is new to `seen`, and the rescan lists it.
#[test]
fn external_create_appears() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    write_file(&cwd.join("existing.md"), "e");

    let (core, _waiter) = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = start_search(&core, wake, true);
    let baseline = poll_until_complete(&search, &wakes);
    assert!(
        displays(&baseline).contains("existing.md") && !displays(&baseline).contains("fresh.md"),
        "sanity: existing.md listed and fresh.md absent before the create; got {:?}",
        displays(&baseline)
    );
    settle();
    let v0 = search.read().version;

    write_file(&cwd.join("fresh.md"), "f");

    let entries = poll_search_until(
        &search,
        &wakes,
        "fresh.md to appear after an external create (a timeout means no watch is live \
         on the root, or its create event never reached the registry)",
        |view| view.entries.iter().any(|e| e.display == "fresh.md"),
    );

    {
        let view = search.read();
        assert!(
            view.version > v0,
            "the watcher-driven swap must bump version past the pre-create value \
             (was {v0}, now {})",
            view.version
        );
        assert!(
            view.complete,
            "the session must stay `complete` across a watcher-driven rescan"
        );
    }
    assert_eq!(
        count_of(&entries, "fresh.md"),
        1,
        "the created file must be listed exactly once; got {:?}",
        displays(&entries)
    );
    assert_eq!(
        count_of(&entries, "existing.md"),
        1,
        "a watcher-driven rescan must not duplicate an already-listed file; got {:?}",
        displays(&entries)
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (b) A file deleted with `std::fs` alone disappears from a live session. The
/// path is in `seen`, so only a remove forwarded as `force` rescans: a remove
/// forwarded as a plain change is skipped by the membership check.
#[test]
fn external_delete_disappears() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    write_file(&cwd.join("doomed.md"), "d");
    write_file(&cwd.join("keeper.md"), "k");

    let (core, _waiter) = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = start_search(&core, wake, true);
    let baseline = poll_until_complete(&search, &wakes);
    assert!(
        displays(&baseline).contains("doomed.md"),
        "sanity: doomed.md should be listed before the external delete; got {:?}",
        displays(&baseline)
    );
    settle();

    fs::remove_file(cwd.join("doomed.md")).expect("failed to remove doomed.md");

    let entries = poll_search_until(
        &search,
        &wakes,
        "doomed.md to disappear after an external delete (a timeout means no watch is \
         live, or the remove was not forwarded as force)",
        |view| !view.entries.iter().any(|e| e.display == "doomed.md"),
    );

    assert!(
        displays(&entries).contains("keeper.md"),
        "the surviving file must still be listed; got {:?}",
        displays(&entries)
    );
    assert!(
        search.read().complete,
        "the session must stay `complete` across a watcher-driven rescan"
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (c) Many writes, creates and removes inside a gitignored directory, and
/// writes to a gitignored root-level file, leave `version` unchanged over a
/// quiet window: the watcher filters each event through the walk's rules and
/// drops every one of these.
///
/// The tree is primed first (see [`prime`]), so any rescan that does run lists
/// `primed.md` and bumps `version`; and the watcher is proven live first (see
/// [`prove_watcher_live`]), so its silence here is not a dead watch.
///
/// NEGATIVE-ASSERTION TIMING: as in (4), the case sleeps [`DEBOUNCE_SETTLE`]
/// after the churn and then samples.
#[test]
fn ignored_churn_leaves_version_unchanged() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    seed_primed_tree(&cwd);

    let (core, _waiter) = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = start_search(&core, wake, true);
    let baseline = poll_until_complete(&search, &wakes);
    let listed = displays(&baseline);
    assert!(
        listed.contains("keep.md") && !listed.contains("build/seed.o"),
        "sanity: keep.md listed and the gitignored build/seed.o not; got {listed:?}"
    );
    assert!(
        !listed.contains("primed.md"),
        "premise: the walk honours `.git/info/exclude` with no git repo (require_git off); \
         primed.md listed means it does not, and priming cannot work; got {listed:?}"
    );
    settle();
    prove_watcher_live(&cwd, &search, &wakes);
    let v0 = prime(&cwd, &search);

    let build = cwd.join("build");
    let sub = build.join("sub");
    fs::create_dir(&sub).expect("failed to create build/sub");
    for i in 0..CHURN_ROUNDS {
        let obj = build.join(format!("churn{i}.o"));
        fs::write(&obj, "a").expect("failed to create a churn object");
        append_to(&obj, "b");
        fs::write(sub.join(format!("deep{i}.o")), "d").expect("failed to write build/sub");
        if i > 0 {
            fs::remove_file(build.join(format!("churn{}.o", i - 1)))
                .expect("failed to remove a churn object");
        }
        append_to(&cwd.join("x.log"), "line\n");
    }
    fs::remove_dir_all(&sub).expect("failed to remove build/sub");
    std::thread::sleep(DEBOUNCE_SETTLE);

    let view = search.read();
    assert_eq!(
        view.version, v0,
        "churn under the gitignored build/ and on the ignored x.log must reach no rescan; a \
         bump means an ignored event was forwarded to the registry (was {v0}, now {})",
        view.version
    );
    assert!(
        !view.entries.iter().any(|e| e.display == "primed.md"),
        "primed.md listed with version unchanged means a rescan swapped the listing \
         without bumping version; got {:?}",
        displays(&view.entries)
    );
    assert!(
        view.entries.iter().any(|e| e.display == "keep.md"),
        "keep.md must stay listed; got {:?}",
        displays(&view.entries)
    );
    drop(view);

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (d) Rewriting and appending to an already-listed file with `std::fs` leaves
/// `version` unchanged: a modify is not `force`, and the path is in `seen`, so
/// the registry's membership check skips it. Primed and proven live as (c) is.
#[test]
fn external_modify_of_listed_file_leaves_version_unchanged() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    seed_primed_tree(&cwd);

    let (core, _waiter) = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = start_search(&core, wake, true);
    let baseline = poll_until_complete(&search, &wakes);
    let listed = displays(&baseline);
    assert!(
        listed.contains("keep.md") && !listed.contains("primed.md"),
        "sanity: keep.md listed and primed.md hidden by `.git/info/exclude`; got {listed:?}"
    );
    settle();
    prove_watcher_live(&cwd, &search, &wakes);
    let v0 = prime(&cwd, &search);

    let keep = cwd.join("keep.md");
    fs::write(&keep, "changed").expect("failed to rewrite keep.md");
    append_to(&keep, "more\n");
    std::thread::sleep(DEBOUNCE_SETTLE);

    let view = search.read();
    assert_eq!(
        view.version, v0,
        "a modify of the listed keep.md must reach no rescan; a bump means a modify was \
         forwarded as force (was {v0}, now {})",
        view.version
    );
    assert!(
        !view.entries.iter().any(|e| e.display == "primed.md"),
        "primed.md listed with version unchanged means a rescan swapped the listing \
         without bumping version; got {:?}",
        displays(&view.entries)
    );
    drop(view);

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (e) An edit to a listed `.gitignore` refilters the listing: appending a rule
/// for `hide.md` makes it disappear. `.gitignore` is itself in `seen`, so only
/// the rule that a `.gitignore` edit forces a rescan can surface it.
#[test]
fn gitignore_edit_refilters_listing() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    write_file(&cwd.join(".gitignore"), "*.tmp\n");
    write_file(&cwd.join("hide.md"), "h");
    write_file(&cwd.join("stay.md"), "s");

    let (core, _waiter) = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = start_search(&core, wake, true);
    let baseline = poll_until_complete(&search, &wakes);
    let listed = displays(&baseline);
    assert!(
        listed.contains(".gitignore") && listed.contains("hide.md") && listed.contains("stay.md"),
        "sanity: .gitignore (so its edit is not new to `seen`), hide.md and stay.md must all \
         be listed; got {listed:?}"
    );
    settle();

    append_to(&cwd.join(".gitignore"), "hide.md\n");

    let entries = poll_search_until(
        &search,
        &wakes,
        "hide.md to disappear after `.gitignore` gained a rule for it (a timeout means the \
         `.gitignore` edit was not forwarded as force)",
        |view| !view.entries.iter().any(|e| e.display == "hide.md"),
    );

    assert!(
        displays(&entries).contains("stay.md"),
        "a file the new rule does not match must stay listed; got {:?}",
        displays(&entries)
    );
    assert!(
        search.read().complete,
        "the session must stay `complete` across a watcher-driven rescan"
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}

/// (f) A listed file renamed into a gitignored directory disappears. The
/// source is in `seen` and the destination is dropped by the filter, so only a
/// rename forwarded as `force` can surface it.
#[test]
fn rename_into_ignored_dir_disappears() {
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();
    write_file(&cwd.join(".gitignore"), "build/\n");
    write_file(&cwd.join("build/seed.o"), "o");
    write_file(&cwd.join("moved.md"), "m");
    write_file(&cwd.join("keeper.md"), "k");

    let (core, _waiter) = start_core_in(&cwd);
    let (wake, wakes) = search_wake();
    let search = start_search(&core, wake, true);
    let baseline = poll_until_complete(&search, &wakes);
    let listed = displays(&baseline);
    assert!(
        listed.contains("moved.md") && !listed.contains("build/seed.o"),
        "sanity: moved.md listed and the gitignored build/seed.o not; got {listed:?}"
    );
    settle();

    fs::rename(cwd.join("moved.md"), cwd.join("build/moved.md"))
        .expect("failed to rename moved.md into build/");

    let entries = poll_search_until(
        &search,
        &wakes,
        "moved.md to disappear after a rename into the ignored build/ (a timeout means the \
         rename was not forwarded as force)",
        |view| {
            !view
                .entries
                .iter()
                .any(|e| e.display == "moved.md" || e.display == "build/moved.md")
        },
    );

    assert!(
        displays(&entries).contains("keeper.md"),
        "the untouched file must still be listed; got {:?}",
        displays(&entries)
    );
    assert!(
        search.read().complete,
        "the session must stay `complete` across a watcher-driven rescan"
    );

    drop(search);
    core.shutdown().expect("shutdown should be Ok");
}
