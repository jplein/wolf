//! Integration test for the `wolf-ffi` file-search bridge surface.
//!
//! This drives the RUST half of the swift-bridge file-search surface against a
//! REAL, hermetic Neovim — NO mocks, per the project testing policy. It is the
//! sibling of `bridge_smoke.rs` (same staging / SKIP-when-unset machinery,
//! reusing `tests/common/mod.rs`) but exercises the NEW picker surface:
//!
//!   * `WolfCoreHandle::start_file_search() -> FfiFileSearch`
//!   * `FfiFileSearch::ranked(query: String, max: usize) -> Vec<FfiFileEntry>`
//!   * `FfiFileEntry::display() -> String` / `FfiFileEntry::path() -> String`
//!   * `WolfCoreHandle::open(path: String)`
//!
//! ## SKIP-when-unset policy
//!
//! Like `bridge_smoke`, this test needs a pinned nvim located via `WOLF_NVIM_BIN`
//! (or the Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is
//! available, via `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.
//!
//! ## What this asserts (and what it does NOT)
//!
//! The strong assertion is `ranked` correctness: a freshly seeded cwd streams its
//! files in, `display()` is the relative filename, `path()` is absolute and under
//! the cwd, and a filtering query ranks the matching file first. `open` is
//! asserted only at the round-trip-survives level: we issue `open(path)` and
//! confirm the core stays responsive (`read_frame` keeps returning a sane frame
//! and does not panic). We CANNOT assert the exact opened buffer name here:
//! `FfiFrame` has no `current_file` accessor and the plan deliberately does NOT
//! add one.

mod common;

use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::wolf_ffi_start;

/// Generous overall timeout for nvim `--embed` startup plus the streaming
/// file-search traversal to surface the seeded files. Mirrors `bridge_smoke`'s
/// `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for the streaming traversal to collect entries.
/// Mirrors `bridge_smoke`'s `POLL_INTERVAL`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Maximum ranked rows requested from `ranked` — mirrors the picker's
/// `PICKER_MAX_ROWS` (well above the two seeded files).
const MAX_RESULTS: usize = 200;

/// The seeded files placed in the temp cwd. Both are top-level, so each file's
/// `display()` must equal its bare filename.
const FILE_ALPHA: &str = "alpha.md";
const FILE_BETA: &str = "beta.md";

/// Number of nested package directories seeded for the large-tree bounding test.
const LARGE_DIR_COUNT: usize = 50;

/// Files placed under each package dir's `src/` in the large-tree test.
const LARGE_FILES_PER_DIR: usize = 100;

/// Of `LARGE_FILES_PER_DIR`, how many are `alphaNNNN.md` (matchable by "alpha").
/// Across `LARGE_DIR_COUNT` dirs this is `LARGE_DIR_COUNT * LARGE_ALPHA_PER_DIR`
/// = 500 alpha files, plus the root `alpha.md` — comfortably more than
/// `MAX_RESULTS`, so bounding to `MAX_RESULTS` rows is observable.
const LARGE_ALPHA_PER_DIR: usize = 10;

/// Returns true iff `needle` is an ASCII-case-insensitive SUBSEQUENCE of
/// `haystack` (the same match relation `rank_matches` uses to keep an entry).
fn is_ci_subsequence(needle: &str, haystack: &str) -> bool {
    let mut hay = haystack.chars();
    for nc in needle.chars() {
        let target = nc.to_ascii_lowercase();
        loop {
            match hay.next() {
                Some(hc) if hc.to_ascii_lowercase() == target => break,
                Some(_) => {}
                None => return false,
            }
        }
    }
    true
}

#[test]
fn file_search_ranks_and_opens() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like
    // `bridge_smoke`. Otherwise export the resolved binary so wolf-core launches
    // it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // Seed a fresh temp cwd with a couple of files for the search to find. We use
    // `tempfile` (a dev-dependency) so the directory is cleaned up on drop; we
    // keep `tmp` alive until after `shutdown()` so the core's `<cwd>/.wolf/` MRU
    // cache outlives the session, then drop it explicitly at the end.
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    std::fs::write(tmp.path().join(FILE_ALPHA), "# alpha\n")
        .expect("failed to seed alpha.md");
    std::fs::write(tmp.path().join(FILE_BETA), "# beta\n")
        .expect("failed to seed beta.md");

    // Canonicalize the cwd for path assertions: on macOS the temp dir under
    // `/var/folders/...` canonicalizes to `/private/var/folders/...`, and
    // wolf-core may report either form, so we compare against the canonical path.
    let canonical_cwd = tmp
        .path()
        .canonicalize()
        .expect("failed to canonicalize temp cwd");

    // Argument order mirrors `bridge_smoke` exactly: (init_lua, cwd, cols, rows).
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    let search = handle.start_file_search();

    // --- ranked("") streams in the seeded files -----------------------------
    // The traversal runs on a background worker, so the first `ranked` call may
    // see an empty (or partial) snapshot; poll until both files appear.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut displays: Vec<String> = Vec::new();
    while Instant::now() < deadline {
        displays = search
            .ranked(String::new(), MAX_RESULTS)
            .iter()
            .map(|e| e.display())
            .collect();
        let both_present = displays.iter().any(|d| d == FILE_ALPHA)
            && displays.iter().any(|d| d == FILE_BETA);
        if both_present {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        displays.iter().any(|d| d == FILE_ALPHA),
        "empty-query results should eventually contain {FILE_ALPHA:?} (got {displays:?})"
    );
    assert!(
        displays.iter().any(|d| d == FILE_BETA),
        "empty-query results should eventually contain {FILE_BETA:?} (got {displays:?})"
    );

    // --- per-entry shape: display() relative, path() absolute + under cwd ----
    let entries = search.ranked(String::new(), MAX_RESULTS);
    assert!(
        !entries.is_empty(),
        "ranked(\"\") should be non-empty after the files have streamed in"
    );
    for entry in &entries {
        let display = entry.display();
        let path = entry.path();
        let path_buf = Path::new(&path);

        assert!(
            path_buf.is_absolute(),
            "path() should be absolute (entry display={display:?}, path={path:?})"
        );
        // The path's final component is the relative display for a top-level file.
        assert!(
            path.ends_with(display.as_str()),
            "path() should end with its display() relative name \
             (display={display:?}, path={path:?})"
        );
        // Robust against the macOS /var -> /private/var symlink: compare the
        // canonicalized path against the canonicalized cwd.
        let canonical_entry = path_buf
            .canonicalize()
            .expect("each ranked entry's path should exist on disk");
        assert!(
            canonical_entry.starts_with(&canonical_cwd),
            "path() should live under the search cwd \
             (path={canonical_entry:?}, cwd={canonical_cwd:?})"
        );
    }

    // --- filtering query ranks the matching file first -----------------------
    // "alpha" matches alpha.md (and not beta.md, which has no a-l-p-h-a
    // subsequence), so alpha.md must be present and, being the best match, first.
    let filtered = search.ranked("alpha".to_string(), MAX_RESULTS);
    let filtered_displays: Vec<String> = filtered.iter().map(|e| e.display()).collect();
    assert!(
        filtered_displays.iter().any(|d| d == FILE_ALPHA),
        "ranked(\"alpha\") should contain {FILE_ALPHA:?} (got {filtered_displays:?})"
    );
    assert_eq!(
        filtered_displays.first().map(String::as_str),
        Some(FILE_ALPHA),
        "ranked(\"alpha\") should rank {FILE_ALPHA:?} first (got {filtered_displays:?})"
    );

    // --- open(path) round-trips without panic; core stays responsive ---------
    // Resolve alpha's absolute path from the entry itself, then open it. We
    // CANNOT assert the opened buffer name (no `current_file` accessor on
    // `FfiFrame`); the assertion is that `open` issues cleanly and the core keeps
    // producing sane frames.
    let alpha_path = entries
        .iter()
        .find(|e| e.display() == FILE_ALPHA)
        .map(|e| e.path())
        .expect("alpha entry should be present in the empty-query results");
    handle.open(alpha_path);

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut frame = handle.read_frame();
    while Instant::now() < deadline {
        frame = handle.read_frame();
        if frame.cols() > 0 && frame.rows() > 0 {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        frame.cols() > 0,
        "core should stay responsive after open() (frame cols, got {})",
        frame.cols()
    );
    assert!(
        frame.rows() > 0,
        "core should stay responsive after open() (frame rows, got {})",
        frame.rows()
    );

    // --- Shutdown: clean teardown --------------------------------------------
    handle.shutdown();

    // Explicitly clean up the temp cwd (and its `.wolf/` MRU cache) now that the
    // session is fully torn down.
    drop(search);
    drop(tmp);
}

/// A LARGE, real-traversal check that `ranked` BOUNDS its output to `max` rows
/// even when far more than `max` entries match the query (the picker
/// performance-fix contract).
///
/// Seeds a temp cwd with a big nested tree (`LARGE_DIR_COUNT` package dirs, each
/// with `LARGE_FILES_PER_DIR` files under `src/`), of which
/// `LARGE_DIR_COUNT * LARGE_ALPHA_PER_DIR` = 500 carry a matchable `alphaNNNN.md`
/// name, plus one exact-best `alpha.md` at the root (501 total "alpha" matches,
/// well above `MAX_RESULTS`). No mocks — a genuine background traversal streams
/// the files in. Asserts:
///   * `ranked("alpha", MAX_RESULTS)` returns EXACTLY `MAX_RESULTS` rows (bounded,
///     even though > `MAX_RESULTS` match), and
///   * the root `alpha.md` (match_start 0, top score) ranks FIRST, and
///   * every returned display contains an `a-l-p-h-a` subsequence (all survivors
///     are genuine matches).
///
/// Follows the same SKIP-when-unset + poll-until-streamed machinery as
/// `file_search_ranks_and_opens`.
#[test]
fn file_search_bounds_large_tree() {
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // Seed a fresh temp cwd with a large nested tree.
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");

    // The exact-best match lives at the root: "alpha.md" has match_start 0 and a
    // component/word-start + contiguous score, so it must outrank the nested
    // "alphaNNNN.md" files (whose "alpha" begins deeper in the display).
    std::fs::write(tmp.path().join(FILE_ALPHA), "# alpha\n").expect("failed to seed root alpha.md");

    let mut alpha_seq = 0usize;
    for dir_idx in 0..LARGE_DIR_COUNT {
        let src_dir = tmp.path().join(format!("pkg{dir_idx:03}")).join("src");
        std::fs::create_dir_all(&src_dir).expect("failed to create nested src dir");
        for file_idx in 0..LARGE_FILES_PER_DIR {
            if file_idx < LARGE_ALPHA_PER_DIR {
                // A matchable "alpha" file.
                let name = format!("alpha{alpha_seq:04}.md");
                std::fs::write(src_dir.join(&name), "x\n").expect("failed to seed alpha file");
                alpha_seq += 1;
            } else {
                // Filler that does NOT contain an a-l-p-h-a subsequence.
                let name = format!("file{file_idx:04}.txt");
                std::fs::write(src_dir.join(&name), "x\n").expect("failed to seed filler file");
            }
        }
    }

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    let search = handle.start_file_search();

    // Poll until enough of the tree has streamed in that "alpha" fills the whole
    // bounded window AND the root alpha.md (the best match) is present and first.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut filtered: Vec<String> = Vec::new();
    while Instant::now() < deadline {
        filtered = search
            .ranked("alpha".to_string(), MAX_RESULTS)
            .iter()
            .map(|e| e.display())
            .collect();
        let full_window = filtered.len() >= MAX_RESULTS;
        let best_first = filtered.first().map(String::as_str) == Some(FILE_ALPHA);
        if full_window && best_first {
            break;
        }
        sleep(POLL_INTERVAL);
    }

    // Bounding: exactly MAX_RESULTS rows even though 501 files match "alpha".
    assert_eq!(
        filtered.len(),
        MAX_RESULTS,
        "ranked(\"alpha\", {MAX_RESULTS}) must be BOUNDED to exactly {MAX_RESULTS} rows \
         even though more than {MAX_RESULTS} files match (got {})",
        filtered.len()
    );

    // The exact-best root alpha.md ranks first.
    assert_eq!(
        filtered.first().map(String::as_str),
        Some(FILE_ALPHA),
        "ranked(\"alpha\") must rank the root {FILE_ALPHA:?} first (got {:?})",
        filtered.first()
    );

    // Every returned display is a genuine a-l-p-h-a subsequence match.
    for display in &filtered {
        assert!(
            is_ci_subsequence("alpha", display),
            "every bounded result must contain an a-l-p-h-a subsequence, but {display:?} does not"
        );
    }

    handle.shutdown();
    drop(search);
    drop(tmp);
}
