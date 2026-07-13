//! Integration test for the `wolf-ffi` file-TREE bridge surface.
//!
//! This drives the RUST half of the swift-bridge file-tree surface against a
//! REAL, hermetic Neovim — NO mocks, per the project testing policy. It is the
//! sibling of `file_search.rs` (same staging / SKIP-when-unset machinery,
//! reusing `tests/common/mod.rs`) but exercises the NEW tree surface added for
//! the macOS file-tree sidebar:
//!
//!   * `WolfCoreHandle::start_tree_search() -> FfiFileSearch`
//!   * `FfiFileSearch::children_of(dir: String) -> Vec<FfiTreeNode>` where
//!     `FfiTreeNode` exposes `name()`, `rel_path()`, `abs_path()`, `is_dir()`
//!   * `FfiFileSearch::rel_path_for(abs: String) -> String` (empty for no match)
//!   * free fn `wolf_ffi_ancestors_to_reveal(rel_path: String) -> Vec<String>`
//!   * `WolfCoreHandle::current_file() -> String` (empty for `None`)
//!   * `WolfCoreHandle::cwd() -> String` (nvim's live, already-canonicalized
//!     working directory; empty until nvim first reports it / after shutdown)
//!
//! ## SKIP-when-unset policy
//!
//! Like `file_search`, this test needs a pinned nvim located via `WOLF_NVIM_BIN`
//! (or the Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is
//! available, via `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.
//!
//! ## What this asserts
//!
//! A freshly seeded, nested cwd is reported by `cwd()` (polled until non-empty,
//! then compared canonicalized to the seeded path). The nested cwd streams its
//! files in; the projected tree lists
//! directories-before-files (case-insensitive), includes dotfiles, and excludes
//! `.gitignore`d files. `rel_path_for` maps a streamed entry's absolute path to
//! its display and yields `""` for an unknown path. `wolf_ffi_ancestors_to_reveal`
//! returns the ancestor dir chain. Finally `open(abs)` is round-tripped: we poll
//! `current_file()` until it equals the opened path (canonicalized, robust to the
//! macOS `/var` -> `/private/var` symlink) and confirm the core stays responsive.

mod common;

use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_ancestors_to_reveal, wolf_ffi_start};

/// Generous overall timeout for nvim `--embed` startup plus the streaming
/// file-search traversal to surface the seeded files. Mirrors `file_search`'s
/// `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for the streaming traversal / `current_file` to
/// settle. Mirrors `file_search`'s `POLL_INTERVAL`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// -- seeded layout ---------------------------------------------------------
//
// A nested tree plus a top-level file, a dotfile (listed), and a `.gitignore`
// + the file it ignores (excluded).
const DIR_SRC: &str = "src";
const DIR_DOCS: &str = "docs";
const FILE_MAIN: &str = "src/main.rs";
const FILE_LIB: &str = "src/lib.rs";
const FILE_INTRO: &str = "docs/intro.md";
const FILE_README: &str = "README.md";
/// A dotfile: `hidden(false)` means the traversal must still list it.
const FILE_DOTFILE: &str = ".config.md";
/// The file named in `.gitignore`; the traversal must EXCLUDE it.
const FILE_IGNORED: &str = "ignored.md";

#[test]
fn tree_search_projects_children_and_reveals_current_file() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like
    // `file_search`. Otherwise export the resolved binary so wolf-core launches
    // it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // Seed a fresh temp cwd with a nested tree, a top-level file, a dotfile, and
    // a `.gitignore` plus the file it ignores. We use `tempfile` (a
    // dev-dependency) so the directory is cleaned up on drop; we keep `tmp` alive
    // until after `shutdown()` so the core's `<cwd>/.wolf/` MRU cache outlives the
    // session, then drop it explicitly at the end.
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    std::fs::create_dir_all(tmp.path().join(DIR_SRC)).expect("failed to create src/");
    std::fs::create_dir_all(tmp.path().join(DIR_DOCS)).expect("failed to create docs/");
    std::fs::write(tmp.path().join(FILE_MAIN), "fn main() {}\n")
        .expect("failed to seed src/main.rs");
    std::fs::write(tmp.path().join(FILE_LIB), "// lib\n").expect("failed to seed src/lib.rs");
    std::fs::write(tmp.path().join(FILE_INTRO), "# intro\n").expect("failed to seed docs/intro.md");
    std::fs::write(tmp.path().join(FILE_README), "# readme\n").expect("failed to seed README.md");
    std::fs::write(tmp.path().join(FILE_DOTFILE), "dotfile\n").expect("failed to seed .config.md");
    std::fs::write(tmp.path().join(".gitignore"), format!("{FILE_IGNORED}\n"))
        .expect("failed to seed .gitignore");
    std::fs::write(tmp.path().join(FILE_IGNORED), "ignored\n").expect("failed to seed ignored.md");

    // Canonicalize the cwd for path assertions: on macOS the temp dir under
    // `/var/folders/...` canonicalizes to `/private/var/folders/...`, and
    // wolf-core may report either form, so we compare against the canonical path.
    let canonical_cwd = tmp
        .path()
        .canonicalize()
        .expect("failed to canonicalize temp cwd");

    // Argument order mirrors `file_search` exactly: (init_lua, cwd, cols, rows).
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- handle.cwd() reports nvim's live, canonical working directory -------
    // `cwd()` is empty until nvim first reports its directory via the
    // `wolf_cwd`/DirChanged notification, which arrives slightly AFTER start, so
    // poll (bounded by the same FLUSH_TIMEOUT/POLL_INTERVAL) until non-empty.
    // Driving a later `:cd` re-root is deferred to manual smoke (flaky headless).
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
        "cwd() should report nvim's live working directory within {FLUSH_TIMEOUT:?} \
         (it stays empty until nvim's first DirChanged/wolf_cwd notification)"
    );
    // Robust against the macOS /var -> /private/var symlink, exactly like the
    // abs_path / current_file comparisons elsewhere in this test: canonicalize
    // the reported cwd and compare it to the canonical seeded cwd.
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

    // --- children_of("") streams in the top level ---------------------------
    // The traversal runs on a background worker, so the first call may see an
    // empty (or partial) snapshot; poll until the dirs and the top-level file
    // appear.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut top = tree.children_of(String::new());
    while Instant::now() < deadline {
        top = tree.children_of(String::new());
        let dirs_present = top.iter().any(|n| n.is_dir() && n.name() == DIR_SRC)
            && top.iter().any(|n| n.is_dir() && n.name() == DIR_DOCS);
        let readme_present = top.iter().any(|n| !n.is_dir() && n.name() == FILE_README);
        if dirs_present && readme_present {
            break;
        }
        sleep(POLL_INTERVAL);
    }

    // --- version() has advanced past its initial state after streaming --------
    // The live snapshot's `version` starts at 1 and is non-decreasing as batches
    // stream in; now that entries have streamed, it must be > 0. Capture it here
    // (as the crate's `u64`) so a later read can prove non-decreasing behavior.
    let v1: u64 = tree.version();
    assert!(
        v1 > 0,
        "version() should be > 0 after entries have streamed in (got {v1})"
    );

    // Directories come BEFORE files, case-insensitively by name. Collect the
    // directory names in order and the file names in order, then assert the
    // dirs all precede the files and each group is correctly ordered.
    let dir_names: Vec<String> = top
        .iter()
        .filter(|n| n.is_dir())
        .map(|n| n.name())
        .collect();
    let file_names: Vec<String> = top
        .iter()
        .filter(|n| !n.is_dir())
        .map(|n| n.name())
        .collect();

    // The two seeded dirs, in case-insensitive name order ("docs" < "src").
    assert_eq!(
        dir_names,
        vec![DIR_DOCS.to_string(), DIR_SRC.to_string()],
        "top-level dirs should be docs, src in case-insensitive order (got {dir_names:?})"
    );

    // All directory nodes must precede all file nodes in the streamed order.
    let first_file_idx = top.iter().position(|n| !n.is_dir());
    let last_dir_idx = top.iter().rposition(|n| n.is_dir());
    if let (Some(first_file), Some(last_dir)) = (first_file_idx, last_dir_idx) {
        assert!(
            last_dir < first_file,
            "every directory must sort before every file (last_dir={last_dir}, \
             first_file={first_file}, nodes={:?})",
            top.iter()
                .map(|n| (n.name(), n.is_dir()))
                .collect::<Vec<_>>()
        );
    }

    // The top-level file AND the dotfile are present (dotfiles are listed).
    assert!(
        file_names.iter().any(|n| n == FILE_README),
        "top level should contain {FILE_README:?} (got files {file_names:?})"
    );
    assert!(
        file_names.iter().any(|n| n == FILE_DOTFILE),
        "top level should contain the dotfile {FILE_DOTFILE:?} (got files {file_names:?})"
    );

    // The gitignored file must be ABSENT at the top level (and is a file name, so
    // it would surface here if listed).
    assert!(
        !top.iter().any(|n| n.name() == FILE_IGNORED),
        "the gitignored file {FILE_IGNORED:?} must not appear in the tree (nodes={:?})",
        top.iter().map(|n| n.name()).collect::<Vec<_>>()
    );
    assert!(
        !top.iter().any(|n| n.rel_path() == FILE_IGNORED),
        "the gitignored file {FILE_IGNORED:?} must not appear by rel_path either"
    );

    // --- children_of("src") yields exactly the two leaf files ---------------
    let src_children = tree.children_of(DIR_SRC.to_string());
    let src_names: Vec<String> = src_children.iter().map(|n| n.name()).collect();
    // Case-insensitive name order: "lib.rs" < "main.rs".
    assert_eq!(
        src_names,
        vec!["lib.rs".to_string(), "main.rs".to_string()],
        "children_of(\"src\") should be exactly lib.rs, main.rs (got {src_names:?})"
    );
    for node in &src_children {
        assert!(
            !node.is_dir(),
            "src children are leaf files; {:?} should not be a dir",
            node.name()
        );
        let rel = node.rel_path();
        assert!(
            rel.starts_with("src/"),
            "child rel_path should start with \"src/\" (got {rel:?})"
        );
        let abs = node.abs_path();
        let abs_path = Path::new(&abs);
        assert!(
            abs_path.is_absolute(),
            "child abs_path should be absolute (got {abs:?})"
        );
        // Robust against the macOS /var -> /private/var symlink: compare the
        // canonicalized path against the canonicalized cwd.
        let canonical_abs = abs_path
            .canonicalize()
            .expect("each streamed child's abs_path should exist on disk");
        assert!(
            canonical_abs.starts_with(&canonical_cwd),
            "child abs_path should live under the cwd (abs={canonical_abs:?}, \
             cwd={canonical_cwd:?})"
        );
    }

    // --- children_of("src") is stable across repeated calls -----------------
    // With no intervening snapshot change, the cached-index path must return
    // identical results across calls. Call twice more and assert equality by
    // (name, rel_path, is_dir) for each node. This is behavioral sanity for the
    // per-snapshot cached index (the build-count proof is headless, in T1).
    let src_children_again_a = tree.children_of(DIR_SRC.to_string());
    let src_children_again_b = tree.children_of(DIR_SRC.to_string());
    let project = |nodes: &[wolf_ffi::FfiTreeNode]| -> Vec<(String, String, bool)> {
        nodes
            .iter()
            .map(|n| (n.name(), n.rel_path(), n.is_dir()))
            .collect()
    };
    assert_eq!(
        project(&src_children_again_a),
        project(&src_children_again_b),
        "two consecutive children_of(\"src\") calls with no snapshot change must \
         return identical (name, rel_path, is_dir) tuples"
    );
    // And they must still match the original streamed children snapshot.
    assert_eq!(
        project(&src_children_again_a),
        project(&src_children),
        "repeated children_of(\"src\") must equal the first snapshot's children"
    );

    // --- version() is non-decreasing across calls with no reset --------------
    // After further reads/settling since `v1`, the snapshot version must never go
    // backwards.
    let v2: u64 = tree.version();
    assert!(
        v2 >= v1,
        "version() must be non-decreasing after more streaming/settling \
         (v1={v1}, later={v2})"
    );
    // And two consecutive reads with no intervening reset stay non-decreasing.
    let v3: u64 = tree.version();
    assert!(
        v3 >= v2,
        "two consecutive version() reads must be non-decreasing (v2={v2}, v3={v3})"
    );

    // --- rel_path_for(abs) maps a streamed entry to its display -------------
    // Use the abs_path of a streamed `src/main.rs` node: it IS that entry's path,
    // so the EXACT-match `rel_path_for` returns its display with `/` separators.
    let main_abs = src_children
        .iter()
        .find(|n| n.name() == "main.rs")
        .map(|n| n.abs_path())
        .expect("src/main.rs should be a streamed child of src");
    assert_eq!(
        tree.rel_path_for(main_abs.clone()),
        FILE_MAIN.to_string(),
        "rel_path_for(<abs of src/main.rs>) should be its display {FILE_MAIN:?}"
    );

    // A non-existent absolute path maps to the empty string.
    let bogus_abs = tmp
        .path()
        .join("does/not/exist.md")
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        tree.rel_path_for(bogus_abs),
        String::new(),
        "rel_path_for(<unknown abs>) should be the empty string"
    );

    // --- wolf_ffi_ancestors_to_reveal ---------------------------------------
    assert_eq!(
        wolf_ffi_ancestors_to_reveal(FILE_MAIN.to_string()),
        vec![DIR_SRC.to_string()],
        "ancestors_to_reveal(\"src/main.rs\") should be [\"src\"]"
    );
    assert_eq!(
        wolf_ffi_ancestors_to_reveal(FILE_README.to_string()),
        Vec::<String>::new(),
        "ancestors_to_reveal(\"README.md\") should be empty"
    );

    // --- open(abs) then poll current_file() until it equals the opened path -
    handle.open(main_abs.clone());

    // Canonicalize the opened path once for comparison.
    let canonical_opened = Path::new(&main_abs)
        .canonicalize()
        .expect("opened path src/main.rs should exist on disk");

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
        "current_file() should equal the opened path after :e \
         (current={current:?}, opened={main_abs:?})"
    );

    // --- core stays responsive after open -----------------------------------
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
        "core should stay responsive after open() (frame cols, got {})",
        frame.cols()
    );

    // --- Shutdown: clean teardown -------------------------------------------
    handle.shutdown();

    // Explicitly clean up the temp cwd (and its `.wolf/` MRU cache) now that the
    // session is fully torn down.
    drop(tree);
    drop(tmp);
}
