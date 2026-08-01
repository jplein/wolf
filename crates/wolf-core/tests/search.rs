//! Integration tests for the ripgrep-backed content search
//! (`wolf_core::WolfCore::start_search` / `SearchSession` / `SearchMatch`) and the
//! open-at-line-and-column round-trip (`WolfCore::open_at`).
//!
//! These drive a REAL, hermetic Neovim (located via `WOLF_NVIM_BIN`, spawned by
//! `WolfCore::start`) AND a REAL, pinned `rg` subprocess (located via
//! `WOLF_RG_BIN`) — NO mocks, consistent with the project's real-tool policy and
//! the sibling `tests/file_search.rs`. The temp trees are seeded with `tempfile`.
//!
//! Surface exercised (per the search plan §2):
//! - `WolfCore::start_search(SearchOptions { query, use_regex, case_sensitive,
//!   all_files }, wake) -> SearchSession` (streams rg matches into a versioned
//!   snapshot behind a lock, sets `complete = true` when rg finishes).
//! - `SearchSession::read() -> SearchReadGuard` derefs to a view exposing
//!   `matches: Vec<SearchMatch>`, `complete: bool`, `version: u64`.
//! - `wolf_core::project(&matches)` (the pure projection) for the SORT assertions,
//!   since `matches` accumulate in rg emission order and are sorted on read.
//! - `WolfCore::open_at(path, line, col)` -> the buffer is edited and the cursor
//!   lands at line/col (observed via `read_snapshot().current_file`).
//!
//! ## rg-availability guard
//!
//! `nvim` is required (the whole wolf-core real-nvim suite panics without it via
//! `common::require_nvim_bin`). `rg` is guarded SEPARATELY by [`rg_bin_or_skip`],
//! which mirrors `common::nvim_bin_or_skip`'s loud-SKIP banner: on a host with no
//! pinned rg staged the test is a no-op pass rather than a failure. The Bazel
//! `rust_test` injects `WOLF_RG_BIN` as `$(rootpath //bazel/rg:rg_tree)/rg/rg`
//! (a RUNFILES-RELATIVE path); [`rg_bin_or_skip`] resolves it to an ABSOLUTE path
//! and re-exports it so `start_search` spawns the right binary regardless of the
//! child's `current_dir`.

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use wolf_core::{project, SearchMatch, SearchOptions, SearchSession, WakeFn, WolfCore};

/// Upper bound for a single content search to reach `complete`. Generous: it
/// includes amortized nvim startup plus the rg subprocess spawn + JSON stream.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(10);

/// How long to block on a wake before re-checking `complete` (so we make progress
/// even if a wake is missed/coalesced).
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// --- Helpers ---------------------------------------------------------------

/// Creates a fresh temp dir to serve as a core `cwd` and returns both the guard
/// (kept alive by the caller) and the CANONICAL path to it (temp dirs can sit
/// behind symlinks like macOS `/tmp -> /private/tmp`; canonicalizing keeps the
/// search root and the relativized `rel_path` strings consistent).
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

/// Resolves a usable pinned `rg` binary as an ABSOLUTE path, mirroring
/// `common::nvim_bin_or_skip`'s resolution + loud-SKIP shape:
///
/// 1. An explicit `WOLF_RG_BIN` that is absolute-and-existing (the cargo / Nix
///    devShell path, where the flake exports `pkgs.ripgrep`).
/// 2. Otherwise, a RUNFILES-RELATIVE `WOLF_RG_BIN` (the Bazel `rust_test`, which
///    sets it to `$(rootpath //bazel/rg:rg_tree)/rg/rg`) resolved against the
///    usual runfiles bases (`TEST_SRCDIR` [+ `TEST_WORKSPACE` / `_main`],
///    `RUNFILES_DIR`, the cwd) — the same probe `common::resolve_runfile` uses.
///
/// Returns `None` after printing a loud SKIP banner when neither resolves.
fn resolve_rg_bin() -> Option<PathBuf> {
    let raw = std::env::var("WOLF_RG_BIN").ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let relp = PathBuf::from(trimmed);
    if relp.is_absolute() {
        return relp.exists().then_some(relp);
    }

    // Bazel: WOLF_RG_BIN is runfiles-relative; probe the known bases.
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Ok(srcdir) = std::env::var("TEST_SRCDIR") {
        if let Ok(wsp) = std::env::var("TEST_WORKSPACE") {
            bases.push(PathBuf::from(&srcdir).join(&wsp));
        }
        bases.push(PathBuf::from(&srcdir).join("_main"));
        bases.push(PathBuf::from(&srcdir));
    }
    if let Ok(rfdir) = std::env::var("RUNFILES_DIR") {
        bases.push(PathBuf::from(&rfdir).join("_main"));
        bases.push(PathBuf::from(&rfdir));
    }
    if let Ok(cwd) = std::env::current_dir() {
        bases.push(cwd);
    }

    bases
        .into_iter()
        .map(|b| b.join(&relp))
        .find(|c| c.exists())
}

/// Resolves `rg` to an ABSOLUTE path and re-exports it into `WOLF_RG_BIN`, or
/// prints a loud SKIP banner and returns `None`. A caller that gets `None` must
/// `return` (a no-op pass), exactly like `common::nvim_bin_or_skip`.
#[must_use]
fn rg_bin_or_skip() -> Option<PathBuf> {
    match resolve_rg_bin() {
        Some(bin) => {
            std::env::set_var("WOLF_RG_BIN", &bin);
            Some(bin)
        }
        None => {
            eprintln!(
                "\n========================================================================\n\
                 SKIP: wolf-core search — no pinned ripgrep (rg) available.\n\
                 This test drives a REAL pinned rg subprocess (no mocks). It runs when\n\
                 WOLF_RG_BIN is an absolute path (Nix/Cargo devShell) or when the Bazel\n\
                 `rust_test` stages //bazel/rg:rg_tree via a runfiles-relative WOLF_RG_BIN.\n\
                 ========================================================================\n"
            );
            None
        }
    }
}

/// A `WakeFn` that signals on a channel, paired with the receiver the poll loop
/// waits on (mirrors `file_search.rs::search_wake`).
fn search_wake() -> (WakeFn, Receiver<()>) {
    let (tx, rx): (Sender<()>, Receiver<()>) = mpsc::channel();
    let wake: WakeFn = Arc::new(move || {
        let _ = tx.send(());
    });
    (wake, rx)
}

/// Starts a `WolfCore` rooted at `cwd` and waits for the initial attach/flush so
/// startup never races the test body.
fn start_core_in(cwd: &Path) -> WolfCore {
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
    waiter.wait_for_initial_flush(&core, common::FLUSH_TIMEOUT);
    core
}

/// Runs a single search to completion and returns the collected `SearchMatch`es
/// (in rg emission order — callers sort via `project` where order matters).
fn run_search(core: &WolfCore, opts: SearchOptions) -> Vec<SearchMatch> {
    let (wake, wakes) = search_wake();
    let session: SearchSession = core.start_search(opts, wake);
    let deadline = Instant::now() + SEARCH_TIMEOUT;
    loop {
        {
            let view = session.read();
            if view.complete {
                let out = view.matches.clone();
                drop(view);
                drop(session);
                return out;
            }
        }
        if Instant::now() >= deadline {
            panic!("content search did not reach `complete` within {SEARCH_TIMEOUT:?}");
        }
        let _ = wakes.recv_timeout(POLL_INTERVAL);
    }
}

/// The set of `rel_path` strings across a slice of matches.
fn rel_paths(matches: &[SearchMatch]) -> BTreeSet<String> {
    matches.iter().map(|m| m.rel_path.clone()).collect()
}

/// The set of `line_number`s across a slice of matches.
fn line_numbers(matches: &[SearchMatch]) -> BTreeSet<u64> {
    matches.iter().map(|m| m.line_number).collect()
}

/// A `SearchOptions` with the given query and the remaining knobs supplied
/// positionally, to cut repetition at call sites.
fn opts(query: &str, use_regex: bool, case_sensitive: bool, all_files: bool) -> SearchOptions {
    SearchOptions {
        query: query.to_string(),
        use_regex,
        case_sensitive,
        all_files,
    }
}

// --- Tests -----------------------------------------------------------------

/// (1) Matches stream in and the session reaches `complete`; the projected file
/// groups are sorted case-insensitively by rel-path; lines within a file are
/// sorted by line number; and a known multi-submatch line yields the correct
/// `match_ranges` + `column`.
#[test]
fn search_streams_sorts_and_reports_ranges() {
    let Some(_rg) = rg_bin_or_skip() else {
        return;
    };
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();

    // A line with the query twice, to exercise multi-submatch range collection.
    let zebra_line = "needle appears twice: needle";
    // apple.md (top-level, lowercase 'a'): matches on lines 1 and 3.
    write_file(
        &cwd.join("apple.md"),
        "the needle is here\nnothing to see\nanother needle line\n",
    );
    // Zebra.md (top-level, uppercase 'Z'): a single line with two submatches.
    write_file(
        &cwd.join("Zebra.md"),
        &format!("no match here\n{zebra_line}\n"),
    );
    // sub/mango.md (nested): a single match.
    write_file(
        &cwd.join("sub").join("mango.md"),
        "a needle in the subdir\n",
    );

    let core = start_core_in(&cwd);
    let matches = run_search(&core, opts("needle", false, false, false));

    assert!(!matches.is_empty(), "the search should surface matches");

    // --- projected groups sorted case-insensitively by rel-path --------------
    // Raw ASCII would put "Zebra.md" (Z=0x5A) before "apple.md" (a=0x61); the
    // case-insensitive projection must instead order apple < sub/mango < Zebra.
    let groups = project(&matches);
    let order: Vec<&str> = groups.iter().map(|g| g.rel_path.as_str()).collect();
    assert_eq!(
        order,
        vec!["apple.md", "sub/mango.md", "Zebra.md"],
        "groups sorted case-insensitively by rel_path (got {order:?})"
    );

    // --- lines within apple.md sorted by line number -------------------------
    let apple = groups
        .iter()
        .find(|g| g.rel_path == "apple.md")
        .expect("apple.md group present");
    let apple_lines: Vec<u64> = apple
        .match_indices
        .iter()
        .map(|&i| matches[i].line_number)
        .collect();
    assert_eq!(
        apple_lines,
        vec![1, 3],
        "apple.md matches ordered by line number (got {apple_lines:?})"
    );

    // --- match_ranges + column for the double-match Zebra line ---------------
    let zebra = matches
        .iter()
        .find(|m| m.rel_path == "Zebra.md")
        .expect("Zebra.md match present");
    assert_eq!(zebra.line_number, 2, "the two-match line is line 2");
    assert_eq!(
        zebra.line_text, zebra_line,
        "line_text is the raw line, newline stripped"
    );

    // Byte offsets computed from the fixture rather than hand-counted.
    let first = zebra_line.find("needle").expect("first needle") as u32;
    let second = zebra_line.rfind("needle").expect("second needle") as u32;
    let needle_len = "needle".len() as u32;
    assert_eq!(
        zebra.match_ranges,
        vec![(first, first + needle_len), (second, second + needle_len)],
        "both submatches collected as byte [start,end) ranges"
    );
    assert_eq!(
        zebra.column, first,
        "column is the first submatch byte start (0-based)"
    );

    // Every match's abs_path is absolute and lives under the (canonical) root.
    for m in &matches {
        assert!(
            m.abs_path.is_absolute(),
            "abs_path should be absolute: {m:?}"
        );
        let canon = m
            .abs_path
            .canonicalize()
            .expect("each match abs_path should exist on disk");
        assert!(
            canon.starts_with(&cwd),
            "match abs_path {canon:?} should live under the root {cwd:?}"
        );
    }

    core.shutdown().expect("shutdown should be Ok");
}

/// (2) `all_files=false` restricts rg to Markdown (`.md`/`.markdown`) and EXCLUDES
/// a matching `.txt`; `all_files=true` removes the glob and INCLUDES it.
#[test]
fn all_files_toggles_the_txt_filter() {
    let Some(_rg) = rg_bin_or_skip() else {
        return;
    };
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();

    write_file(&cwd.join("keep.md"), "needle in markdown\n");
    write_file(&cwd.join("keep.markdown"), "needle in markdown ext\n");
    write_file(&cwd.join("notes.txt"), "needle in plain text\n");

    let core = start_core_in(&cwd);

    // Default (Markdown only): the .txt must not appear, but both markdown exts do.
    let md_only = rel_paths(&run_search(&core, opts("needle", false, false, false)));
    assert!(
        md_only.contains("keep.md"),
        "Markdown-only search should include keep.md; got {md_only:?}"
    );
    assert!(
        md_only.contains("keep.markdown"),
        "Markdown-only search should include keep.markdown; got {md_only:?}"
    );
    assert!(
        !md_only.contains("notes.txt"),
        "Markdown-only search must EXCLUDE notes.txt; got {md_only:?}"
    );

    // all_files: the .txt is now included.
    let all = rel_paths(&run_search(&core, opts("needle", false, false, true)));
    assert!(
        all.contains("notes.txt"),
        "all-files search must INCLUDE notes.txt; got {all:?}"
    );
    assert!(
        all.contains("keep.md"),
        "all-files search still includes markdown; got {all:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// (3) `use_regex=false` treats the query as a LITERAL (a regex metachar matches
/// itself); `use_regex=true` honors a real regex.
#[test]
fn use_regex_literal_vs_regex() {
    let Some(_rg) = rg_bin_or_skip() else {
        return;
    };
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();

    // Line 1 contains the literal "a.c"; line 2 contains "abc" (no literal dot).
    write_file(
        &cwd.join("regex.md"),
        "literal a.c here\nalphabet abc end\n",
    );

    let core = start_core_in(&cwd);

    // Literal: "a.c" matches ONLY the line containing the literal dot (line 1).
    let literal = line_numbers(&run_search(&core, opts("a.c", false, false, false)));
    assert_eq!(
        literal,
        BTreeSet::from([1]),
        "literal a.c matches only the literal-dot line; got {literal:?}"
    );

    // Regex: "a.c" ('.' = any char) matches BOTH "a.c" (line 1) and "abc" (line 2).
    let regex = line_numbers(&run_search(&core, opts("a.c", true, false, false)));
    assert_eq!(
        regex,
        BTreeSet::from([1, 2]),
        "regex a.c matches both a.c and abc; got {regex:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// (4) `case_sensitive` flips matching on a mixed-case query: true matches only the
/// exact case, false matches both.
#[test]
fn case_sensitive_differs_on_mixed_case() {
    let Some(_rg) = rg_bin_or_skip() else {
        return;
    };
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();

    // Line 1 has capitalized "Needle"; line 2 has lowercase "needle".
    write_file(
        &cwd.join("case.md"),
        "Needle upper case\nneedle lower case\n",
    );

    let core = start_core_in(&cwd);

    // Case-sensitive (lowercase query): only the lowercase line (line 2) matches.
    let sensitive = line_numbers(&run_search(&core, opts("needle", false, true, false)));
    assert_eq!(
        sensitive,
        BTreeSet::from([2]),
        "case-sensitive 'needle' matches only the lowercase line; got {sensitive:?}"
    );

    // Case-insensitive: both lines match.
    let insensitive = line_numbers(&run_search(&core, opts("needle", false, false, false)));
    assert_eq!(
        insensitive,
        BTreeSet::from([1, 2]),
        "case-insensitive 'needle' matches both lines; got {insensitive:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// (5) `open_at` round-trip: opening a matched file at a line/col makes it the
/// current buffer (observed via `read_snapshot().current_file`), and the core
/// stays responsive afterward.
#[test]
fn open_at_round_trip() {
    let Some(_rg) = rg_bin_or_skip() else {
        return;
    };
    let _ = common::require_nvim_bin();
    let (_dir, cwd) = temp_cwd();

    let target = cwd.join("open_me.md");
    write_file(
        &target,
        "first line\nsecond line has a needle\nthird line\n",
    );

    let core = start_core_in(&cwd);

    // Find the match so we open at a real line/col (rg line 1-based, col 0-based).
    let matches = run_search(&core, opts("needle", false, false, false));
    let m = matches
        .iter()
        .find(|m| m.rel_path == "open_me.md")
        .expect("open_me.md match present");
    let line = m.line_number;
    let col = u64::from(m.column);

    let canonical_opened = target
        .canonicalize()
        .expect("target file should exist on disk");

    core.open_at(target.clone(), line, col);

    // Poll current_file() until it equals the opened path (canonicalized, robust to
    // the macOS /var -> /private/var symlink).
    let deadline = Instant::now() + SEARCH_TIMEOUT;
    let mut current: Option<PathBuf> = None;
    while Instant::now() < deadline {
        current = core.read_snapshot().current_file.clone();
        if let Some(ref path) = current {
            if let Ok(canon) = path.canonicalize() {
                if canon == canonical_opened {
                    break;
                }
            }
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    let current = current.expect("current_file should be set after open_at");
    let canonical_current = current.canonicalize().unwrap_or_else(|_| current.clone());
    assert_eq!(
        canonical_current, canonical_opened,
        "open_at should make the matched file the current buffer \
         (current={current:?}, opened={target:?})"
    );

    // Core stays responsive: the snapshot advanced past the initial attach flush.
    assert!(
        core.read_snapshot().seq >= 1,
        "core should stay responsive after open_at (snapshot seq >= 1)"
    );

    core.shutdown().expect("shutdown should be Ok");
}
