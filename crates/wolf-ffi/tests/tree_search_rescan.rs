//! End-to-end test for the fs-event-driven RESCAN of the file-TREE bridge
//! surface (the macOS sidebar-staleness bug fix): a long-lived
//! `start_tree_search()` session must reflect a file created through the
//! captive Neovim (`:e fresh.md`, insert, `:w`) and a file deleted from disk
//! (`remove_file` + `:bdelete!`), with `version()` advancing on each swap and
//! the directories-before-files projection intact.
//!
//! Modeled on `tree_search.rs` (same staging / SKIP-when-unset machinery,
//! reusing `tests/common/mod.rs`, same canonical-cwd comparisons and bounded
//! poll loops). The keystrokes ride the same `send_key` / `FfiKeyEvent` path
//! the generated Swift glue uses, mirroring `cmdline_ui.rs`.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim located via `WOLF_NVIM_BIN`
//! (or the Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is
//! available, via `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.

mod common;

use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent, WolfCoreHandle};

/// Generous overall timeout for nvim `--embed` startup plus the streaming
/// traversal AND each debounced rescan (default 200 ms debounce + a tiny-tree
/// walk). Mirrors `tree_search.rs`'s `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for the traversal / a rescan to surface. Mirrors
/// `tree_search.rs`'s `POLL_INTERVAL`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;
/// The Esc key (leaves insert mode).
const KIND_ESC: u8 = 2;

// -- seeded layout ----------------------------------------------------------
//
// One directory plus one top-level file, so the final-ordering assertion has
// both node kinds to order, and the delete flow has survivors to check.
const DIR_DOCS: &str = "docs";
const FILE_INTRO: &str = "docs/intro.md";
const FILE_README: &str = "README.md";
/// The file created THROUGH nvim after the baseline settles, then deleted.
const FILE_FRESH: &str = "fresh.md";

/// Builds a printable-character `FfiKeyEvent` (kind=Char) for `c`, all
/// modifiers false. Mirrors `cmdline_ui.rs`.
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

/// Builds a modifier-less `FfiKeyEvent` for a non-character key `kind`.
fn plain_key(kind: u8) -> FfiKeyEvent {
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

/// Types each char of `s` through the same `send_key` / `FfiKeyEvent` path the
/// generated Swift glue uses.
fn type_str(handle: &WolfCoreHandle, s: &str) {
    for c in s.chars() {
        handle.send_key(char_key(c));
    }
}

/// Types `cmd` as an Ex command and presses Enter. `cmd` must include the
/// leading `:`. Mirrors `cmdline_ui.rs::send_ex_command`.
fn send_ex_command(handle: &WolfCoreHandle, cmd: &str) {
    type_str(handle, cmd);
    handle.send_key(plain_key(KIND_ENTER));
}

#[test]
fn tree_search_rescan_reflects_write_and_delete() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like
    // `tree_search`. Otherwise export the resolved binary so wolf-core launches
    // it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // Seed a fresh temp cwd; keep `tmp` alive until after `shutdown()` (it is
    // nvim's working dir + search root), then drop it explicitly at the end.
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    std::fs::create_dir_all(tmp.path().join(DIR_DOCS)).expect("failed to create docs/");
    std::fs::write(tmp.path().join(FILE_INTRO), "# intro\n").expect("failed to seed docs/intro.md");
    std::fs::write(tmp.path().join(FILE_README), "# readme\n").expect("failed to seed README.md");

    // Canonicalize the cwd for path comparisons: on macOS the temp dir under
    // `/var/folders/...` canonicalizes to `/private/var/folders/...`, and
    // wolf-core may report either form.
    let canonical_cwd = tmp
        .path()
        .canonicalize()
        .expect("failed to canonicalize temp cwd");

    // Argument order mirrors `tree_search` exactly: (init_lua, cwd, cols, rows).
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // Wait for nvim to settle before driving keys: `cwd()` is empty until nvim's
    // first DirChanged/wolf_cwd notification, so poll it non-empty (the same
    // readiness gate `tree_search` uses) and pin it to the seeded cwd.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut reported_cwd = handle.cwd();
    while Instant::now() < deadline {
        reported_cwd = handle.cwd();
        if !reported_cwd.is_empty() {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        !reported_cwd.is_empty(),
        "cwd() should report nvim's live working directory within {FLUSH_TIMEOUT:?}"
    );
    let canonical_reported = Path::new(&reported_cwd)
        .canonicalize()
        .expect("cwd() should report a path that exists on disk");
    assert_eq!(
        canonical_reported, canonical_cwd,
        "cwd() should equal the seeded cwd after nvim settles its directory \
         (reported={reported_cwd:?}, seeded={canonical_cwd:?})"
    );

    // The tree gets its OWN long-lived `FfiFileSearch` via `start_tree_search()`.
    let tree = handle.start_tree_search();

    // --- baseline: root children stream in ----------------------------------
    // The traversal runs on a background worker; poll until the seeded dir and
    // the top-level file appear.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut top = tree.children_of(String::new());
    while Instant::now() < deadline {
        top = tree.children_of(String::new());
        let docs_present = top.iter().any(|n| n.is_dir() && n.name() == DIR_DOCS);
        let readme_present = top.iter().any(|n| !n.is_dir() && n.name() == FILE_README);
        if docs_present && readme_present {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        top.iter().any(|n| n.is_dir() && n.name() == DIR_DOCS)
            && top.iter().any(|n| !n.is_dir() && n.name() == FILE_README),
        "baseline root children should hold docs/ and README.md within \
         {FLUSH_TIMEOUT:?} (got {:?})",
        top.iter()
            .map(|n| (n.name(), n.is_dir()))
            .collect::<Vec<_>>()
    );

    // Capture the version BEFORE the write, so the rescan's swap is provable as
    // an advance past this point.
    let v_pre: u64 = tree.version();

    // --- write flow: `:e fresh.md`, insert, `:w` through the bridge ---------
    // The `:w` fires BufWritePost -> wolf_fs_changed -> debounced rescan, which
    // must surface a new file node at the root.
    send_ex_command(&handle, &format!(":e {FILE_FRESH}"));
    handle.send_key(char_key('i'));
    type_str(&handle, "hello");
    handle.send_key(plain_key(KIND_ESC));
    send_ex_command(&handle, ":w");

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut fresh_seen = false;
    while Instant::now() < deadline {
        top = tree.children_of(String::new());
        fresh_seen = top.iter().any(|n| !n.is_dir() && n.name() == FILE_FRESH);
        // Require BOTH the node and the version advance before declaring the
        // rescan landed: the version is what wakes the Swift side into
        // `scheduleRefresh()`, so a node without a version bump would still be
        // an invisible update in the real UI.
        if fresh_seen && tree.version() > v_pre {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        fresh_seen,
        "a file node {FILE_FRESH:?} should appear at the root after :w \
         (got {:?})",
        top.iter()
            .map(|n| (n.name(), n.is_dir()))
            .collect::<Vec<_>>()
    );
    let v_post = tree.version();
    assert!(
        v_post > v_pre,
        "version() must advance past the pre-write value once the rescan swaps \
         (pre={v_pre}, post={v_post})"
    );
    let fresh_node = top
        .iter()
        .find(|n| n.name() == FILE_FRESH)
        .expect("fresh.md node was just asserted present");
    assert_eq!(
        fresh_node.rel_path(),
        FILE_FRESH,
        "the new top-level file's rel_path should be its bare name"
    );

    // --- delete flow: remove from disk, then `:bdelete!` ---------------------
    // Remove the file BEFORE dropping the buffer so the BufDelete-triggered
    // rescan walks a tree that no longer holds it.
    std::fs::remove_file(tmp.path().join(FILE_FRESH)).expect("failed to remove fresh.md");
    send_ex_command(&handle, ":bdelete!");

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut fresh_gone = false;
    while Instant::now() < deadline {
        top = tree.children_of(String::new());
        fresh_gone = !top.iter().any(|n| n.name() == FILE_FRESH);
        if fresh_gone {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        fresh_gone,
        "the {FILE_FRESH:?} node should disappear after remove_file + :bdelete! \
         (got {:?})",
        top.iter()
            .map(|n| (n.name(), n.is_dir()))
            .collect::<Vec<_>>()
    );

    // --- the projection invariants survive the swaps -------------------------
    // The survivors are all still there…
    assert!(
        top.iter().any(|n| n.is_dir() && n.name() == DIR_DOCS),
        "docs/ must survive the rescans (got {:?})",
        top.iter().map(|n| n.name()).collect::<Vec<_>>()
    );
    assert!(
        top.iter().any(|n| !n.is_dir() && n.name() == FILE_README),
        "README.md must survive the rescans (got {:?})",
        top.iter().map(|n| n.name()).collect::<Vec<_>>()
    );
    // …and every directory node still precedes every file node, exactly as the
    // initial-stream ordering assertion in `tree_search` demands.
    let first_file_idx = top.iter().position(|n| !n.is_dir());
    let last_dir_idx = top.iter().rposition(|n| n.is_dir());
    if let (Some(first_file), Some(last_dir)) = (first_file_idx, last_dir_idx) {
        assert!(
            last_dir < first_file,
            "directories must still sort before files after the rescan swaps \
             (last_dir={last_dir}, first_file={first_file}, nodes={:?})",
            top.iter()
                .map(|n| (n.name(), n.is_dir()))
                .collect::<Vec<_>>()
        );
    }

    // --- Shutdown: clean teardown -------------------------------------------
    handle.shutdown();
    drop(tree);
    drop(tmp);
}
