//! Integration test for the `wolf-ffi` MARKDOWN-PREVIEW bridge surface (plan
//! §"FFI" + the "Real-nvim FFI" test bullet): the "is the active buffer markdown"
//! gate, the `open_preview(mode)` mechanism (single-pane split-right vs. multi-pane
//! active-pane, OD3), the `previews()` snapshot of `FfiPreview` rows, the blocking
//! `render_preview(source_bufnr)` HTML pull, and the fact that the preview's
//! scratch buffer surfaces in the buffer picker as `Preview: <basename>` (OD1).
//!
//! This drives the RUST half against a REAL, hermetic Neovim — NO mocks, per the
//! project testing policy. It is modeled on `tests/buffer_picker.rs` and
//! `tests/pane_status.rs`, reusing `tests/common/mod.rs` and the same loud
//! SKIP-when-no-nvim guard.
//!
//! ## NOTE to coder — Bazel target
//!
//! This file needs a `//crates/wolf-ffi:preview` `rust_test` target added to
//! `crates/wolf-ffi/BUILD.bazel` (a copy of the `:buffer_picker` stanza:
//! `srcs = ["tests/common/mod.rs", "tests/preview.rs"]`,
//! `crate_root = "tests/preview.rs"`). This test file does NOT touch BUILD files.
//!
//! ## Dependency on coder work (plan §FFI)
//!
//! None of this exists yet — the coder adds, on `WolfCoreHandle`:
//!   * `is_markdown_active(&self) -> bool`
//!   * `open_preview(&self, mode: String) -> i64` (preview bufnr, 0 on
//!     error/not-markdown)
//!   * `render_preview(&self, source_bufnr: i64) -> String` (blocking; empty on
//!     error)
//!   * `previews(&self) -> Vec<FfiPreview>`
//! plus the opaque `FfiPreview { win() -> i64, source_bufnr() -> i64,
//! source_name() -> String (basename), source_path() -> String,
//! source_tick() -> u64 }` in `crates/wolf-ffi/src/lib.rs`, regenerates
//! `generated/`, and appends `command_kind::PREVIEW = 18`. This test file will not
//! compile until that lands — expected.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim (`WOLF_NVIM_BIN` or the
//! Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is available.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent, FfiPreview};

/// Generous overall timeout for nvim `--embed` startup plus each async effect
/// (filetype detection, the split + `nvim_win_set_buf`, the out-of-band
/// `wolf_preview` / `wolf_markdown_dirty` notifications) to settle. Mirrors the
/// sibling tests' `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface. Mirrors the siblings.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// A short settle between window-cycling `:wincmd w` attempts, long enough for the
/// `WinEnter`/`BufEnter` autocmd to re-emit `wolf_current_file` (so
/// `is_markdown_active()` reflects the newly-focused window).
const CYCLE_SETTLE: Duration = Duration::from_millis(150);

/// A `max` large enough that the assembled buffer set is never truncated.
const RANKED_MAX: usize = 100;

/// The lowest real WINDOW grid id (grid 1 is the outer/global grid; editor windows
/// are `>= 2` under `ext_multigrid`). Mirrors `pane_status.rs`.
const WINDOW_GRID_MIN: u64 = 2;

/// The two-pane window count reached after the split-right preview placement.
const TWO_PANES: usize = 2;

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

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

/// The live editor WINDOW grids (grids `>= WINDOW_GRID_MIN` that are not hidden).
/// Mirrors `pane_status.rs::live_windows`.
fn live_window_count(handle: &wolf_ffi::WolfCoreHandle) -> usize {
    handle
        .windows()
        .iter()
        .filter(|w| !w.hidden() && w.grid() >= WINDOW_GRID_MIN)
        .count()
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
    assert_eq!(got, target, "is_markdown_active() should be {target} ({desc})");
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

/// Blocks until `predicate(&previews)` holds on a fresh `previews()` snapshot,
/// returning that snapshot. Panics with `desc` on timeout.
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

/// The `source_tick()` of the preview row whose `source_bufnr()` is `bufnr`, or
/// `None` if no such row is present in this snapshot.
fn tick_for_source(previews: &[FfiPreview], bufnr: i64) -> Option<u64> {
    previews
        .iter()
        .find(|p| p.source_bufnr() == bufnr)
        .map(FfiPreview::source_tick)
}

/// Blocks (bounded by [`FLUSH_TIMEOUT`]) until the core's `current_file()` ends
/// with `suffix` (a BASENAME match, robust to absolute-vs-cwd-relative display and
/// to `/var`↔`/private/var` symlinking), deterministically confirming that file is
/// the ACTIVE buffer. Unlike `is_markdown_active` — which stays true across a
/// markdown→markdown `:e` and so cannot witness the switch — this only flips once
/// the `wolf_current_file` notification for the new buffer has landed. Panics with
/// `suffix` on timeout.
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

/// Cycles editor windows with `:wincmd w` until the active window reports a
/// markdown buffer (`is_markdown_active()`), so a subsequent edit lands on the
/// SOURCE pane rather than the (non-markdown) preview scratch pane. With exactly
/// two panes this needs at most one cycle.
fn focus_markdown_source(handle: &wolf_ffi::WolfCoreHandle) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        if handle.is_markdown_active() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out cycling windows to focus the markdown source pane"
        );
        send_ex_command(handle, ":wincmd w");
        sleep(CYCLE_SETTLE);
    }
}

#[test]
fn preview_gates_opens_lists_and_live_updates() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // A fresh temp cwd holding a markdown file (heading + paragraph) and a plain
    // text file (the non-markdown gate case), kept alive until after `shutdown()`.
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    std::fs::write(
        tmp.path().join("alpha.md"),
        "# Alpha Heading\n\nSome paragraph text.\n",
    )
    .expect("failed to write alpha.md");
    std::fs::write(tmp.path().join("notes.txt"), "plain text notes\n")
        .expect("failed to write notes.txt");

    // Argument order mirrors the sibling tests exactly: (init_lua, cwd, cols, rows).
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Attach -----------------------------------------------------------
    wait_for_attach(&handle);

    // --- 2. The "is markdown" gate follows the active buffer's filetype -------
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "after :e alpha.md");

    send_ex_command(&handle, ":e notes.txt");
    wait_for_markdown_active(&handle, false, "after :e notes.txt");

    // Back on the markdown buffer for the preview.
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "back on alpha.md");

    // --- 3. Single-pane placement: open_preview("split_right") ---------------
    // One pane at this point, so the preview opens in a NEW pane to the right.
    // `open_preview` returns the preview scratch bufnr (> 0 since markdown active).
    let preview_bufnr = handle.open_preview("split_right".to_string());
    assert!(
        preview_bufnr > 0,
        "open_preview(\"split_right\") on a markdown buffer must return a valid \
         preview bufnr (> 0), got {preview_bufnr}"
    );
    wait_for_window_count(&handle, TWO_PANES, "after split-right preview");

    // --- 4. previews() surfaces one row for alpha.md -------------------------
    let previews = wait_for_previews(&handle, "one alpha.md preview", |p| {
        p.iter().any(|row| row.source_name() == "alpha.md")
    });
    let alpha_row = previews
        .iter()
        .find(|row| row.source_name() == "alpha.md")
        .expect("a preview row for alpha.md must exist");
    let alpha_bufnr = alpha_row.source_bufnr();
    assert!(
        alpha_bufnr > 0,
        "the preview row must carry alpha.md's real source bufnr (> 0), got {alpha_bufnr}"
    );

    // --- 5. render_preview() returns themed HTML with the heading + markers ---
    let html = handle.render_preview(alpha_bufnr);
    assert!(
        html.contains("Alpha Heading"),
        "render_preview must contain the source heading text: {html}"
    );
    assert!(
        html.contains(r#"id="wl-1-0""#),
        "render_preview must carry the first hard line's `wl-1-0` anchor: {html}"
    );
    // The heading slug id ships through the nvim -> render_preview path (proves the
    // `HeadingIdAssigner` output survives the FFI round-trip, not just the pure
    // `render_html` unit tests). `# Alpha Heading` slugs to `alpha-heading`.
    assert!(
        html.contains(r#"id="alpha-heading""#),
        "render_preview must carry the heading slug id from the nvim path: {html}"
    );

    // --- 6. Live-update: editing the source advances the preview's tick -------
    // Focus the SOURCE pane (the split left the preview pane active), capture the
    // current tick, delete a line (`dd` — a normal-mode buffer change that fires
    // TextChanged), and observe the joined source_tick advance.
    focus_markdown_source(&handle);
    let initial_tick = tick_for_source(&handle.previews(), alpha_bufnr).unwrap_or(0);

    handle.send_key(char_key('d'));
    handle.send_key(char_key('d'));

    wait_for_previews(&handle, "source_tick advances after an edit", |p| {
        tick_for_source(p, alpha_bufnr).is_some_and(|t| t > initial_tick)
    });

    // --- 6b. The preview row exposes the source's cwd-relative display path ---
    // `source_display_path()` feeds the preview pane's SECONDARY statusbar line
    // (CHANGE 3), mirroring editor panes. The fixture lives at the cwd ROOT, so the
    // display path ends with the bare basename. Use `ends_with`, NOT exact equality:
    // the source path is uncanonicalized while live_cwd is canonical, so a
    // `/var`<->`/private/var` mismatch can leave `source_display_path()` absolute.
    let alpha_display = alpha_row.source_display_path();
    assert!(
        !alpha_display.is_empty(),
        "the preview row must expose a non-empty source_display_path(), got {alpha_display:?}"
    );
    assert!(
        alpha_display.ends_with("alpha.md"),
        "source_display_path() must end with the source file path (alpha.md), got {alpha_display}"
    );

    // --- 7. Buffer picker lists the preview as `Preview: alpha.md` -----------
    // A fresh picker snapshots list_buffers(); the buflisted scratch buffer shows
    // up with its `Preview: <basename>` name (OD1) with NO front-end injection.
    let rows = handle.buffer_picker().ranked(String::new(), RANKED_MAX);
    assert!(
        rows.iter()
            .any(|r| r.display().starts_with("Preview: alpha.md")),
        "the buffer picker must include a `Preview: alpha.md` row: {:?}",
        rows.iter().map(|r| r.display()).collect::<Vec<_>>()
    );

    // --- 8. Multi-pane placement: open_preview("active") ---------------------
    // We are focused on the source pane with two panes live, so "active" replaces
    // THIS pane's buffer with the preview scratch (nvim_win_set_buf). Both panes
    // then show a preview-source buffer, so previews() reports two rows.
    let reused = handle.open_preview("active".to_string());
    assert_eq!(
        reused, preview_bufnr,
        "open_preview(\"active\") must REUSE the same scratch buffer for this source"
    );
    wait_for_previews(&handle, "both panes show the preview buffer", |p| {
        p.len() == TWO_PANES
    });

    // --- 9. Shutdown: handle calls return neutral values ---------------------
    handle.shutdown();
    assert!(
        !handle.is_markdown_active(),
        "after shutdown is_markdown_active() must be the neutral false"
    );
    assert!(
        handle.previews().is_empty(),
        "after shutdown previews() must be the neutral empty vec"
    );
    assert!(
        handle.render_preview(alpha_bufnr).is_empty(),
        "after shutdown render_preview() must be the neutral empty string"
    );
    assert_eq!(
        handle.open_preview("active".to_string()),
        0,
        "after shutdown open_preview() must be the neutral 0"
    );

    drop(tmp);
}

/// Seeds a fresh temp cwd with `alpha.md` AND `beta.md` (both real Markdown), the
/// two source files the reuse/fallback tests edit. Returns the live `TempDir` (kept
/// alive until after `shutdown()`) so its `.wolf/` MRU cache outlives the core.
fn seed_two_markdown_files() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    std::fs::write(
        tmp.path().join("alpha.md"),
        "# Alpha Heading\n\nSome paragraph text.\n",
    )
    .expect("failed to write alpha.md");
    std::fs::write(
        tmp.path().join("beta.md"),
        "# Beta Heading\n\nAnother paragraph.\n",
    )
    .expect("failed to write beta.md");
    tmp
}

#[test]
fn preview_picker_shows_cwd_relative_path_for_subdir_source() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // Seed the markdown source in a SUBDIRECTORY of the cwd. The picker names the
    // preview scratch buffer with the cwd-relative path (`:.` -> `docs/alpha.md`),
    // which is distinct from the bare basename (`:t` -> `alpha.md`) the old naming
    // produced — this is the ONLY test that witnesses the `:.` change (CHANGE 1).
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    let docs_dir = tmp.path().join("docs");
    std::fs::create_dir(&docs_dir).expect("failed to create docs subdir");
    std::fs::write(
        docs_dir.join("alpha.md"),
        "# Alpha Heading\n\nSome paragraph text.\n",
    )
    .expect("failed to write docs/alpha.md");

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Attach; open docs/alpha.md (in the subdir) as the source ---------
    wait_for_attach(&handle);
    send_ex_command(&handle, ":e docs/alpha.md");
    wait_for_markdown_active(&handle, true, "after :e docs/alpha.md");

    // --- 2. Split-right preview: previews() surfaces one row for alpha.md -----
    let preview_bufnr = handle.open_preview("split_right".to_string());
    assert!(
        preview_bufnr > 0,
        "open_preview(\"split_right\") on a markdown buffer must return a valid \
         preview bufnr (> 0), got {preview_bufnr}"
    );
    wait_for_window_count(&handle, TWO_PANES, "after split-right preview");
    // `source_name()` is the BASENAME, so the row is found by `alpha.md` regardless
    // of the subdirectory it lives in.
    let previews = wait_for_previews(&handle, "one docs/alpha.md preview", |p| {
        p.iter().any(|row| row.source_name() == "alpha.md")
    });
    let alpha_row = previews
        .iter()
        .find(|row| row.source_name() == "alpha.md")
        .expect("a preview row for docs/alpha.md must exist");

    // --- 3. The preview row's display path is the cwd-relative subdir path ----
    // `ends_with` (not exact equality): uncanonicalized source path vs canonical
    // live_cwd can leave this absolute on a `/var`<->`/private/var` mismatch.
    let display = alpha_row.source_display_path();
    assert!(
        display.ends_with("docs/alpha.md"),
        "source_display_path() must be the cwd-relative subdir path docs/alpha.md, got {display}"
    );

    // --- 4. The picker names the scratch buffer with the cwd-relative path -----
    // `:.` yields `Preview: docs/alpha.md`; the old `:t` would have yielded the bare
    // `Preview: alpha.md`. This distinguishes the new naming from the old.
    let rows = handle.buffer_picker().ranked(String::new(), RANKED_MAX);
    assert!(
        rows.iter()
            .any(|r| r.display().starts_with("Preview: docs/alpha.md")),
        "the buffer picker must include a `Preview: docs/alpha.md` row: {:?}",
        rows.iter().map(|r| r.display()).collect::<Vec<_>>()
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn preview_reuses_the_other_preview_pane() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();
    let tmp = seed_two_markdown_files();

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Attach; open alpha.md as the source ------------------------------
    wait_for_attach(&handle);
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "after :e alpha.md");

    // --- 2. Split-right preview: two panes, right previews alpha.md ----------
    let alpha_preview_bufnr = handle.open_preview("split_right".to_string());
    assert!(
        alpha_preview_bufnr > 0,
        "open_preview(\"split_right\") on a markdown buffer must return a valid \
         preview bufnr (> 0), got {alpha_preview_bufnr}"
    );
    wait_for_window_count(&handle, TWO_PANES, "after split-right preview");
    let previews = wait_for_previews(&handle, "one alpha.md preview", |p| {
        p.iter().any(|row| row.source_name() == "alpha.md")
    });
    let preview_win = previews
        .iter()
        .find(|row| row.source_name() == "alpha.md")
        .expect("a preview row for alpha.md must exist")
        .win();
    assert!(
        preview_win > 0,
        "the alpha.md preview must carry a real preview WINDOW handle (> 0), got {preview_win}"
    );

    // --- 3. Focus the SOURCE pane and edit beta.md there ---------------------
    // The split left the preview pane active; cycle back to the markdown source
    // pane, then :e beta.md IN THAT source window (not the preview pane).
    focus_markdown_source(&handle);
    let active_before = handle.active_win();
    send_ex_command(&handle, ":e beta.md");
    // is_markdown_active is ALREADY true (the pane still shows alpha.md, also
    // markdown), so it cannot witness this markdown→markdown switch. Wait on
    // current_file() instead so beta.md is genuinely the ACTIVE buffer before the
    // blocking reuse call reads the current source.
    wait_for_current_file(&handle, "beta.md");

    // --- 4. Reuse placement: bind beta's preview into the OTHER preview pane --
    let beta_preview_bufnr = handle.open_preview("reuse_preview_pane".to_string());
    assert!(
        beta_preview_bufnr > 0,
        "open_preview(\"reuse_preview_pane\") on a markdown buffer must return a \
         valid preview bufnr (> 0), got {beta_preview_bufnr}"
    );

    // --- 5. Assert: beta reused the SAME preview window; nothing else moved ---
    // The deterministic emit at the end of wolf_open_preview replaces the whole
    // previews snapshot, so wait for exactly one row: beta.md at `preview_win`.
    let previews = wait_for_previews(
        &handle,
        "the sole preview row is beta.md bound into the reused preview window",
        |p| {
            p.len() == 1
                && p.iter()
                    .any(|row| row.source_name() == "beta.md" && row.win() == preview_win)
        },
    );
    assert_eq!(
        previews.len(),
        1,
        "reuse must WHOLESALE-replace the preview: exactly one row (beta.md), got {previews:?}",
    );
    assert!(
        !previews.iter().any(|row| row.source_name() == "alpha.md"),
        "alpha.md's preview row must be gone after the reuse rebind: {previews:?}",
    );
    assert_eq!(
        live_window_count(&handle),
        TWO_PANES,
        "reuse must NOT create a new pane (still two live windows)",
    );
    assert_eq!(
        handle.active_win(),
        active_before,
        "reuse must NOT change the active window (the source pane stays focused)",
    );
    // The reuse rebind does a background-window `nvim_win_set_buf`, after which
    // `wolf_emit_current_file` re-asserts the ACTIVE (source) buffer's filetype
    // asynchronously via the redraw/notification path. Reading
    // `is_markdown_active()` synchronously here races that emit, so poll (bounded
    // by FLUSH_TIMEOUT, the same convention as the other `wait_for_*` helpers)
    // until it settles to true — the active source pane still shows markdown.
    wait_for_markdown_active(
        &handle,
        true,
        "the active (source) pane still shows a markdown buffer after the reuse rebind",
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn preview_reuse_falls_back_to_active_when_no_other_preview() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();
    let tmp = seed_two_markdown_files();

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Attach; open alpha.md as the source ------------------------------
    wait_for_attach(&handle);
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "after :e alpha.md");

    // --- 2. Make a second EDITOR pane (no previews) via :vsplit --------------
    // Both panes now show the SAME markdown buffer; neither is a preview pane.
    send_ex_command(&handle, ":vsplit");
    wait_for_window_count(&handle, TWO_PANES, "after :vsplit into a second editor pane");
    focus_markdown_source(&handle);
    assert!(
        handle.previews().is_empty(),
        "no preview windows should exist before the fallback open_preview: {:?}",
        handle.previews(),
    );
    let active_before = handle.active_win();

    // --- 3. Reuse placement with NO other preview -> fall back to active -----
    // wolf_open_preview finds no OTHER window carrying b:wolf_preview_source, so the
    // Lua fallback binds into the CURRENT (active) window.
    let preview_bufnr = handle.open_preview("reuse_preview_pane".to_string());
    assert!(
        preview_bufnr > 0,
        "open_preview(\"reuse_preview_pane\") on a markdown buffer must return a \
         valid preview bufnr (> 0), got {preview_bufnr}"
    );

    // --- 4. Assert: exactly one preview, bound into the ACTIVE window --------
    let previews = wait_for_previews(
        &handle,
        "one preview row bound into the active (fallback) window",
        |p| p.len() == 1 && p.iter().any(|row| row.win() == active_before),
    );
    assert_eq!(
        previews.len(),
        1,
        "the fallback must produce exactly one preview row, got {previews:?}",
    );
    assert_eq!(
        previews[0].win(),
        active_before,
        "with no OTHER preview window the fallback binds into the ACTIVE window",
    );
    assert_eq!(
        live_window_count(&handle),
        TWO_PANES,
        "the fallback must NOT create a new pane (still two live windows)",
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn preview_file_repoints_pane_to_linked_file() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, like the siblings.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // A fresh temp cwd holding alpha.md (which LINKS to beta.md at an anchor), the
    // linked beta.md, and a plain-text file (the non-markdown `preview_file` gate
    // case). Kept alive until after `shutdown()`.
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    std::fs::write(
        tmp.path().join("alpha.md"),
        "# Alpha Heading\n\n[to beta](beta.md#beta-heading)\n",
    )
    .expect("failed to write alpha.md");
    std::fs::write(
        tmp.path().join("beta.md"),
        "# Beta Heading\n\nAnother paragraph.\n",
    )
    .expect("failed to write beta.md");
    std::fs::write(tmp.path().join("notes.txt"), "plain text notes\n")
        .expect("failed to write notes.txt");

    // Absolute paths (as WebKit would resolve a `file://` link; Lua does NO
    // relative resolution). `tmp.path()` is already absolute.
    let beta_path = tmp
        .path()
        .join("beta.md")
        .to_string_lossy()
        .into_owned();
    let txt_path = tmp
        .path()
        .join("notes.txt")
        .to_string_lossy()
        .into_owned();

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Attach; open alpha.md as the source and split-right preview it ----
    wait_for_attach(&handle);
    send_ex_command(&handle, ":e alpha.md");
    wait_for_markdown_active(&handle, true, "after :e alpha.md");

    let alpha_preview_bufnr = handle.open_preview("split_right".to_string());
    assert!(
        alpha_preview_bufnr > 0,
        "open_preview(\"split_right\") on a markdown buffer must return a valid \
         preview bufnr (> 0), got {alpha_preview_bufnr}"
    );
    wait_for_window_count(&handle, TWO_PANES, "after split-right preview");
    let previews = wait_for_previews(&handle, "one alpha.md preview", |p| {
        p.iter().any(|row| row.source_name() == "alpha.md")
    });
    let preview_win = previews
        .iter()
        .find(|row| row.source_name() == "alpha.md")
        .expect("a preview row for alpha.md must exist")
        .win();
    assert!(
        preview_win > 0,
        "the alpha.md preview must carry a real preview WINDOW handle (> 0), got {preview_win}"
    );

    // --- 2. preview_file repoints that preview WINDOW at beta.md --------------
    // Returns beta.md's SOURCE bufnr (> 0); it must NOT open a new pane.
    let beta_bufnr = handle.preview_file(preview_win, beta_path.clone());
    assert!(
        beta_bufnr > 0,
        "preview_file(win, <abs beta.md>) must return beta.md's source bufnr (> 0), got {beta_bufnr}"
    );

    // The preview row for THIS window now reports beta.md (a re-emit replaces the
    // snapshot), and the live window count is unchanged (view-only repoint).
    wait_for_previews(
        &handle,
        "the preview window now previews beta.md",
        |p| {
            p.iter()
                .any(|row| row.win() == preview_win && row.source_name() == "beta.md")
        },
    );
    assert_eq!(
        live_window_count(&handle),
        TWO_PANES,
        "preview_file must NOT create a new pane (still two live windows)",
    );

    // --- 3. render_preview(beta) carries beta's heading text + slug id --------
    let html = handle.render_preview(beta_bufnr);
    assert!(
        html.contains("Beta Heading"),
        "render_preview(beta) must contain the linked file's heading text: {html}"
    );
    assert!(
        html.contains(r#"id="beta-heading""#),
        "render_preview(beta) must carry beta's heading slug id: {html}"
    );

    // --- 4. Negative: a non-markdown link returns 0 and changes nothing -------
    let before = handle.previews();
    let non_md = handle.preview_file(preview_win, txt_path.clone());
    assert_eq!(
        non_md, 0,
        "preview_file(win, <abs .txt>) must return the neutral 0 (non-markdown gate)"
    );
    // The preview window still previews beta.md (the failed call left it alone).
    let after = handle.previews();
    assert!(
        after
            .iter()
            .any(|row| row.win() == preview_win && row.source_name() == "beta.md"),
        "a rejected non-markdown preview_file must leave the previews() snapshot unchanged: \
         before {:?} after {:?}",
        before
            .iter()
            .map(|r| (r.win(), r.source_name()))
            .collect::<Vec<_>>(),
        after
            .iter()
            .map(|r| (r.win(), r.source_name()))
            .collect::<Vec<_>>(),
    );

    // --- 5. Shutdown: preview_file returns the neutral 0 ---------------------
    handle.shutdown();
    assert_eq!(
        handle.preview_file(preview_win, beta_path.clone()),
        0,
        "after shutdown preview_file() must be the neutral 0"
    );

    drop(tmp);
}
