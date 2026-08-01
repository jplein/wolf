//! Integration test for the `wolf-ffi` CONTENT-SEARCH bridge surface.
//!
//! This drives the RUST half of the swift-bridge search surface against a REAL,
//! hermetic Neovim AND a REAL, pinned `rg` subprocess — NO mocks, per the project
//! testing policy. It is the sibling of `tree_search.rs` (same staging /
//! SKIP-when-unset machinery, reusing `tests/common/mod.rs`) but exercises the NEW
//! search surface added for the macOS sidebar search panel:
//!
//!   * `WolfCoreHandle::start_search(query, use_regex, case_sensitive, all_files)
//!     -> FfiSearch`
//!   * `FfiSearch::version() -> u64`, `FfiSearch::files(budget) -> Vec<FfiSearchFile>`
//!   * `FfiSearchFile::label()` / `rel_path()` / `abs_path()` / `matches()`
//!   * `FfiSearchMatch::line_number()` / `column()` / `display_text()` /
//!     `truncated_left()` / `truncated_right()` / `range_starts()` / `range_ends()`
//!   * `WolfCoreHandle::ignorecase() -> bool`
//!   * `WolfCoreHandle::open_at(path, line, col)` (round-tripped via `current_file`)
//!
//! ## SKIP-when-unset policy
//!
//! Like `tree_search`, this test needs a pinned nvim (`WOLF_NVIM_BIN` /
//! `WOLF_TEST_NVIM_DIR`) — resolved via `common::nvim_bin_or_skip` — AND a pinned
//! `rg` (`WOLF_RG_BIN`), resolved by the local [`rg_bin_or_skip`]. It **loudly
//! SKIPS** (no-op pass) when either is unavailable. The Bazel `rust_test` injects
//! `WOLF_RG_BIN` as `$(rootpath //bazel/rg:rg_tree)/rg/rg` (RUNFILES-RELATIVE);
//! [`rg_bin_or_skip`] resolves it to an ABSOLUTE path and re-exports it so the
//! bridged `start_search` spawns the right binary regardless of the child's cwd.

mod common;

use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::wolf_ffi_start;

/// Generous overall timeout for nvim `--embed` startup plus the rg subprocess to
/// surface the seeded matches. Mirrors `tree_search`'s `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for the streaming search / `current_file` to settle.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Character budget passed to `files(budget)`. Comfortably larger than every
/// seeded line's length, so `display_text` is the whole line and neither side
/// truncates. (The truncation math itself is unit-tested headlessly in
/// wolf-core's `search_projection` target.)
const CHAR_BUDGET: u32 = 200;

// -- seeded layout ---------------------------------------------------------
const FILE_ALPHA: &str = "alpha.md";
const FILE_BETA: &str = "sub/beta.md";
const FILE_NOTES: &str = "notes.txt";
/// The query, present in all three files; the .txt is filtered out unless
/// `all_files` is set.
const QUERY: &str = "needle";

/// Resolves a runfiles-relative or absolute path against the usual bases, mirroring
/// `common::resolve_runfile` (which is private to the common module).
fn resolve_runfile_value(value: &str) -> Option<PathBuf> {
    let relp = PathBuf::from(value);
    if relp.is_absolute() {
        return relp.exists().then_some(relp);
    }
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
/// prints a loud SKIP banner and returns `None`. Mirrors `common::nvim_bin_or_skip`.
#[must_use]
fn rg_bin_or_skip() -> Option<PathBuf> {
    if let Ok(raw) = std::env::var("WOLF_RG_BIN") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            if let Some(bin) = resolve_runfile_value(trimmed) {
                std::env::set_var("WOLF_RG_BIN", &bin);
                return Some(bin);
            }
        }
    }
    eprintln!(
        "\n========================================================================\n\
         SKIP: wolf-ffi search — no pinned ripgrep (rg) available.\n\
         This test drives a REAL pinned rg subprocess through the FFI bridge.\n\
         It runs when WOLF_RG_BIN is set (Nix/Cargo devShell) or when the Bazel\n\
         `rust_test` stages //bazel/rg:rg_tree via WOLF_RG_BIN.\n\
         ========================================================================\n"
    );
    None
}

#[test]
fn search_projects_files_matches_and_opens_at_line() {
    // Loud SKIP (no-op pass) when either the pinned nvim or the pinned rg is
    // unavailable.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    let Some(_rg_bin) = rg_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // Seed a fresh temp cwd: a top-level markdown file, a nested markdown file, and
    // a plain-text file (excluded unless all_files). Keep `tmp` alive until after
    // shutdown so it outlives the session as nvim's cwd + search root.
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    std::fs::create_dir_all(tmp.path().join("sub")).expect("failed to create sub/");
    std::fs::write(tmp.path().join(FILE_ALPHA), "needle one\n").expect("seed alpha.md");
    std::fs::write(tmp.path().join(FILE_BETA), "the needle two\n").expect("seed sub/beta.md");
    std::fs::write(tmp.path().join(FILE_NOTES), "needle in text\n").expect("seed notes.txt");

    let canonical_cwd = tmp
        .path()
        .canonicalize()
        .expect("failed to canonicalize temp cwd");

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- ignorecase(): the captive init.lua sets no ignorecase, so nvim's default
    // (off) applies. Value is one-shot (seeds the case toggle) and stable across
    // reads.
    let ic1 = handle.ignorecase();
    let ic2 = handle.ignorecase();
    assert_eq!(ic1, ic2, "ignorecase() should be stable across reads");
    assert!(
        !ic1,
        "the captive nvim sets no ignorecase, so its default (off) is reported"
    );

    // --- start_search (Markdown only): stream until both markdown files appear ---
    let search = handle.start_search(QUERY.to_string(), false, false, false);

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut files = search.files(CHAR_BUDGET);
    while Instant::now() < deadline {
        files = search.files(CHAR_BUDGET);
        let alpha = files.iter().any(|f| f.rel_path() == FILE_ALPHA);
        let beta = files.iter().any(|f| f.rel_path() == FILE_BETA);
        if alpha && beta {
            break;
        }
        sleep(POLL_INTERVAL);
    }

    // version() advances past its initial state once matches have streamed in.
    let v1 = search.version();
    assert!(
        v1 > 0,
        "version() should be > 0 after matches stream in (got {v1})"
    );

    // The .txt is filtered out under the default Markdown-only glob.
    let rels: Vec<String> = files.iter().map(|f| f.rel_path()).collect();
    assert!(
        rels.iter().any(|r| r == FILE_ALPHA),
        "Markdown-only search should include {FILE_ALPHA:?}; got {rels:?}"
    );
    assert!(
        rels.iter().any(|r| r == FILE_BETA),
        "Markdown-only search should include {FILE_BETA:?}; got {rels:?}"
    );
    assert!(
        !rels.iter().any(|r| r == FILE_NOTES),
        "Markdown-only search must EXCLUDE {FILE_NOTES:?}; got {rels:?}"
    );

    // Groups sorted case-insensitively by rel-path: "alpha.md" < "sub/beta.md".
    assert_eq!(
        rels,
        vec![FILE_ALPHA.to_string(), FILE_BETA.to_string()],
        "files sorted by rel_path (got {rels:?})"
    );

    // --- FfiSearchFile / FfiSearchMatch accessors for alpha.md ---------------
    let alpha = files
        .iter()
        .find(|f| f.rel_path() == FILE_ALPHA)
        .expect("alpha.md file present");

    // label() is "<basename> - <parent>", or just the basename for a root file.
    assert_eq!(
        alpha.label(),
        "alpha.md",
        "a root file's label is just its basename"
    );

    // abs_path() is absolute and lives under the (canonical) cwd.
    let alpha_abs = alpha.abs_path();
    let alpha_abs_path = Path::new(&alpha_abs);
    assert!(
        alpha_abs_path.is_absolute(),
        "abs_path should be absolute (got {alpha_abs:?})"
    );
    let canonical_alpha = alpha_abs_path
        .canonicalize()
        .expect("alpha.md abs_path should exist on disk");
    assert!(
        canonical_alpha.starts_with(&canonical_cwd),
        "alpha abs_path should live under the cwd (abs={canonical_alpha:?}, cwd={canonical_cwd:?})"
    );

    let alpha_matches = alpha.matches();
    assert_eq!(
        alpha_matches.len(),
        1,
        "alpha.md has exactly one matching line"
    );
    let m = &alpha_matches[0];

    assert_eq!(m.line_number(), 1, "the match is on line 1");
    assert_eq!(m.column(), 0, "'needle' starts at column 0 (0-based byte)");
    assert!(
        m.display_text().contains(QUERY),
        "display_text should carry the matched line (got {:?})",
        m.display_text()
    );
    assert!(
        !m.truncated_left(),
        "a short line does not truncate on the left"
    );
    assert!(
        !m.truncated_right(),
        "a short line does not truncate on the right"
    );

    // The bold range covers exactly the matched "needle" at [0,6).
    let starts = m.range_starts();
    let ends = m.range_ends();
    assert_eq!(
        starts.len(),
        ends.len(),
        "range_starts/range_ends are parallel"
    );
    assert_eq!(starts, vec![0], "one bold range starting at byte 0");
    assert_eq!(
        ends,
        vec![QUERY.len() as u32],
        "the bold range ends at the query length"
    );

    // The bold range indexes into display_text and covers the query.
    let text = m.display_text();
    let (s, e) = (starts[0] as usize, ends[0] as usize);
    assert!(e <= text.len(), "bold range end within display_text bounds");
    assert_eq!(
        &text[s..e],
        QUERY,
        "the bold range spans the query in display_text"
    );

    // --- all_files: the .txt is now included --------------------------------
    let all_search = handle.start_search(QUERY.to_string(), false, false, true);
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut all_rels: Vec<String> = Vec::new();
    while Instant::now() < deadline {
        all_rels = all_search
            .files(CHAR_BUDGET)
            .iter()
            .map(|f| f.rel_path())
            .collect();
        if all_rels.iter().any(|r| r == FILE_NOTES) {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        all_rels.iter().any(|r| r == FILE_NOTES),
        "all-files search must INCLUDE {FILE_NOTES:?}; got {all_rels:?}"
    );

    // --- open_at(abs, line, col) round-trip ---------------------------------
    handle.open_at(alpha_abs.clone(), m.line_number(), u64::from(m.column()));

    let canonical_opened = Path::new(&alpha_abs)
        .canonicalize()
        .expect("opened path alpha.md should exist on disk");

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut current = handle.current_file();
    while Instant::now() < deadline {
        current = handle.current_file();
        if !current.is_empty() {
            if let Ok(canonical_current) = Path::new(&current).canonicalize() {
                if canonical_current == canonical_opened {
                    break;
                }
            }
        }
        sleep(POLL_INTERVAL);
    }
    let canonical_current = Path::new(&current)
        .canonicalize()
        .unwrap_or_else(|_| Path::new(&current).to_path_buf());
    assert_eq!(
        canonical_current, canonical_opened,
        "current_file() should equal the opened path after open_at \
         (current={current:?}, opened={alpha_abs:?})"
    );

    // --- core stays responsive after open_at --------------------------------
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
        "core should stay responsive after open_at (frame cols, got {})",
        frame.cols()
    );

    handle.shutdown();
    drop(search);
    drop(all_search);
    drop(tmp);
}
