//! Pure, headless tests for the buffer-picker candidate assembly + ranking
//! (`wolf_core::rank_buffers` and the `BufInfo` / `BufferRow` value types). No
//! mocks, no Neovim, no filesystem — `rank_buffers` is IO-free, exactly like
//! `rank::rank_matches` and `palette::rank_palette` which it reuses/mirrors.
//!
//! These live in a dedicated integration file (with its own `buffer_rank` BUILD
//! target) rather than an in-module `#[cfg(test)] mod tests` inside the NEW
//! `src/buffer_picker.rs`, to avoid colliding with the coder who creates that
//! file. This mirrors the crate's existing precedent for `palette_rank.rs`.
//!
//! ## Dependency on coder work (plan §4 / §8a)
//!
//! None of this exists yet — the coder adds `pub mod buffer_picker;` (+ the
//! re-exports this file imports) to `crates/wolf-core/src/lib.rs`, and adds the
//! `//crates/wolf-core:buffer_rank` `rust_test` target to `BUILD.bazel` (a copy
//! of the `palette_rank` stanza). This file does NOT touch non-test source. It
//! assumes the plan's exact signatures:
//!
//! - `pub struct BufInfo { pub bufnr: i64, pub name: String, pub lastused: i64,
//!   pub modified: bool }` (derives `Debug` / `Clone` for the panic messages here)
//! - `pub struct BufferRow { pub bufnr: i64, pub display: String,
//!   pub modified: bool }` (derives `Debug` for the panic messages here)
//! - `#[must_use] pub fn rank_buffers(buffers: &[BufInfo], cwd: Option<&Path>,
//!   query: &str, max: usize) -> Vec<BufferRow>`
//!
//! Display contract (plan §4, DECISION §13.6 — no `~`-home fold in v1):
//!   empty name -> `[No Name]`; a name under `cwd` -> the cwd-relative remainder;
//!   a name outside `cwd` -> the absolute path verbatim.

use std::path::Path;

use wolf_core::{rank_buffers, BufInfo, BufferRow, NO_NAME};

/// A fixed, on-disk-free cwd so no filesystem is ever touched (all `name`s are
/// plain absolute-path strings arithmetic'd against this prefix).
const CWD: &str = "/tmp/proj";

/// A `max` large enough to never truncate the assembled set in membership /
/// ordering tests (a handful of buffers stays well under this).
const A_LARGE_MAX: usize = 100;

/// Distinct `lastused` timestamps, oldest -> newest. Values >= 5 live in named
/// constants to keep the magic-number lint quiet.
const LASTUSED_OLD: i64 = 10;
const LASTUSED_MID: i64 = 20;
const LASTUSED_NEW: i64 = 30;

/// A shared `lastused` used by the two tie-break buffers (they collide, so the
/// tie must break on `bufnr` ascending).
const LASTUSED_TIE: i64 = 15;

/// Builds an unmodified [`BufInfo`] (the common case for ordering tests).
fn buf(bufnr: i64, name: &str, lastused: i64) -> BufInfo {
    BufInfo {
        bufnr,
        name: name.to_string(),
        lastused,
        modified: false,
    }
}

/// The cwd used across the ordering tests, as a `Path`.
fn cwd() -> &'static Path {
    Path::new(CWD)
}

/// Finds the ranked row (if any) for `bufnr`.
fn row_for_bufnr(rows: &[BufferRow], bufnr: i64) -> Option<&BufferRow> {
    rows.iter().find(|r| r.bufnr == bufnr)
}

/// Position of the ranked row for `bufnr`, or `None`.
fn pos_of_bufnr(rows: &[BufferRow], bufnr: i64) -> Option<usize> {
    rows.iter().position(|r| r.bufnr == bufnr)
}

/// (a) An empty query returns every buffer in `lastused`-DESCending (MRU) order,
/// with the highest-`lastused` (current) buffer at index 0.
#[test]
fn empty_query_orders_by_lastused_desc_current_first() {
    // Deliberately shuffled input order so the sort — not the input — decides.
    let buffers = [
        buf(1, "/tmp/proj/old.md", LASTUSED_OLD),
        buf(2, "/tmp/proj/new.md", LASTUSED_NEW),
        buf(3, "/tmp/proj/mid.md", LASTUSED_MID),
    ];

    let rows = rank_buffers(&buffers, Some(cwd()), "", A_LARGE_MAX);

    let order: Vec<i64> = rows.iter().map(|r| r.bufnr).collect();
    assert_eq!(
        order,
        vec![2, 3, 1],
        "empty query must yield lastused-DESC (MRU) order, current (highest) first: {rows:?}"
    );
    assert_eq!(
        rows.first().map(|r| r.bufnr),
        Some(2),
        "the highest-lastused (current) buffer must be at index 0"
    );
}

/// (b) Equal `lastused` breaks deterministically on `bufnr` ASCending.
#[test]
fn equal_lastused_breaks_tie_by_bufnr_ascending() {
    // Input order puts the higher bufnr FIRST so a stable sort alone would not
    // reorder it — the tie-break must actively put bufnr 2 ahead of bufnr 4.
    let buffers = [
        buf(4, "/tmp/proj/four.md", LASTUSED_TIE),
        buf(2, "/tmp/proj/two.md", LASTUSED_TIE),
    ];

    let rows = rank_buffers(&buffers, Some(cwd()), "", A_LARGE_MAX);

    let order: Vec<i64> = rows.iter().map(|r| r.bufnr).collect();
    assert_eq!(
        order,
        vec![2, 4],
        "equal lastused must break on bufnr ASCending (2 before 4): {rows:?}"
    );
}

/// (c) A non-empty query narrows to the matching displays and keeps MRU-first
/// order among the survivors. Query "rs":
///   - "main.rs" / "lib.rs" contain the subsequence r..s -> KEPT.
///   - "README.md" has no 's' after its 'r' -> DROPPED.
/// Among the survivors, the higher-`lastused` (lib.rs) sorts ahead of main.rs.
#[test]
fn query_narrows_and_keeps_mru_first() {
    let buffers = [
        buf(1, "/tmp/proj/main.rs", LASTUSED_OLD),
        buf(2, "/tmp/proj/lib.rs", LASTUSED_NEW),
        buf(3, "/tmp/proj/README.md", LASTUSED_MID),
    ];

    let rows = rank_buffers(&buffers, Some(cwd()), "rs", A_LARGE_MAX);

    assert!(
        row_for_bufnr(&rows, 3).is_none(),
        "README.md matches neither r..s and must be dropped: {rows:?}"
    );
    let p_lib = pos_of_bufnr(&rows, 2).expect("lib.rs must survive the 'rs' filter");
    let p_main = pos_of_bufnr(&rows, 1).expect("main.rs must survive the 'rs' filter");
    assert!(
        p_lib < p_main,
        "among matches, the more-recently-used lib.rs must rank ahead of main.rs \
         (lib@{p_lib}, main@{p_main})"
    );
    assert_eq!(
        rows.len(),
        2,
        "exactly the two '.rs' buffers survive: {rows:?}"
    );
}

/// (d) Display strings: a name UNDER cwd -> the relative remainder; a name
/// OUTSIDE cwd -> the absolute path verbatim (no `~` fold); an EMPTY name ->
/// `[No Name]`. Also confirms the `modified` flag rides through onto the row.
#[test]
fn display_is_cwd_relative_absolute_or_no_name() {
    let buffers = [
        buf(1, "/tmp/proj/src/main.rs", LASTUSED_NEW),
        BufInfo {
            bufnr: 2,
            name: "/etc/hosts".to_string(),
            lastused: LASTUSED_MID,
            modified: true,
        },
        buf(3, "", LASTUSED_OLD),
    ];

    let rows = rank_buffers(&buffers, Some(cwd()), "", A_LARGE_MAX);

    let under = row_for_bufnr(&rows, 1).expect("the under-cwd buffer must be present");
    assert_eq!(
        under.display, "src/main.rs",
        "a name under cwd displays as the cwd-relative remainder"
    );
    assert!(!under.modified, "the under-cwd buffer is unmodified");

    let outside = row_for_bufnr(&rows, 2).expect("the outside-cwd buffer must be present");
    assert_eq!(
        outside.display, "/etc/hosts",
        "a name outside cwd displays as the absolute path verbatim (no ~ fold)"
    );
    assert!(
        outside.modified,
        "the modified flag must ride through onto the row"
    );

    let unnamed = row_for_bufnr(&rows, 3).expect("the unnamed buffer must be present");
    assert_eq!(
        unnamed.display, NO_NAME,
        "an empty name displays as the [No Name] placeholder"
    );
}

/// (e) `max` truncates the ranked output to at most `max` rows.
#[test]
fn max_truncates_output() {
    let buffers = [
        buf(1, "/tmp/proj/a.md", LASTUSED_OLD),
        buf(2, "/tmp/proj/b.md", LASTUSED_MID),
        buf(3, "/tmp/proj/c.md", LASTUSED_NEW),
    ];

    let truncated = 2;
    let rows = rank_buffers(&buffers, Some(cwd()), "", truncated);
    assert_eq!(
        rows.len(),
        truncated,
        "max must truncate the assembled candidate set to {truncated} rows"
    );
    // Truncation keeps the MRU-first head: c.md (newest) then b.md.
    let order: Vec<i64> = rows.iter().map(|r| r.bufnr).collect();
    assert_eq!(
        order,
        vec![3, 2],
        "truncation keeps the MRU-first head (c.md, b.md): {rows:?}"
    );
}
