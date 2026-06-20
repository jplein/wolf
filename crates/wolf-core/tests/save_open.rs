//! Tests for app-level file semantics delegated to nvim: opening an existing
//! markdown file (with a syntax-highlight smoke check), saving the current
//! buffer, and saving to a new path. Requires a real nvim via `WOLF_NVIM_BIN`.

mod common;

use std::time::Duration;

use common::{ch, key, FLUSH_TIMEOUT};
use wolf_core::{InputEvent, Key};

/// Heading content used by the open + highlight test.
const HEADING_DOC: &str = "# Title\nbody text here\n";

/// Text typed into the save tests.
const SAVE_TEXT: &str = "saved content line";

/// How long to wait for a `write`/`saveas` to land on disk before re-checking.
/// Save is delegated to nvim and the file appears asynchronously.
const SAVE_POLL_TIMEOUT: Duration = Duration::from_secs(10);
const SAVE_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Polls `path` until it exists and contains `needle`, or panics after timeout.
fn wait_for_file_contains(path: &std::path::Path, needle: &str) -> String {
    let deadline = std::time::Instant::now() + SAVE_POLL_TIMEOUT;
    loop {
        if let Ok(contents) = std::fs::read_to_string(path) {
            if contents.contains(needle) {
                return contents;
            }
        }
        if std::time::Instant::now() >= deadline {
            let dump = std::fs::read_to_string(path).unwrap_or_else(|e| format!("<unreadable: {e}>"));
            panic!(
                "timed out waiting for {} to contain {needle:?}; current contents: {dump:?}",
                path.display()
            );
        }
        std::thread::sleep(SAVE_POLL_INTERVAL);
    }
}

#[test]
fn open_existing_markdown() {
    let (core, _waiter, _file) = common::start_core_with_temp_md(HEADING_DOC);

    let snap = core.read_snapshot();

    let row0 = snap.grid.row_text(0);
    assert!(
        row0.contains("# Title"),
        "row 0 should contain the heading text; got {row0:?}"
    );

    // Basic syntax-highlight smoke test: the heading row should carry a different
    // hl_id from a plain body cell. Markdown highlighting is enabled in init.lua.
    // Pick the first non-space cell on the heading row (row 0) and on the body
    // row (row 1) and assert their hl_ids differ.
    let heading_col = first_non_space_col(&row0)
        .expect("heading row should have a non-space char");
    let heading_cell = snap
        .grid
        .cell(0, heading_col)
        .expect("expected a heading cell");

    let row1 = snap.grid.row_text(1);
    let body_col = first_non_space_col(&row1).expect("body row should have a non-space char");
    let body_cell = snap.grid.cell(1, body_col).expect("expected a body cell");

    assert_ne!(
        heading_cell.hl_id, body_cell.hl_id,
        "heading cell hl_id ({}) should differ from body cell hl_id ({}) \
         (markdown syntax highlighting)",
        heading_cell.hl_id, body_cell.hl_id
    );

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn save_writes_file() {
    // Open an empty, named .md so `Save` (nvim `write`) has a target path.
    let (core, waiter, file) = common::start_core_with_temp_md("");

    core.send_input(ch('i'));
    common::type_str(&core, SAVE_TEXT);
    core.send_input(key(Key::Esc));
    waiter.wait_for_flush(FLUSH_TIMEOUT);

    core.send_input(InputEvent::Save);

    let contents = wait_for_file_contains(file.path(), SAVE_TEXT);
    assert!(
        contents.contains(SAVE_TEXT),
        "saved file should contain the typed text; got {contents:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

#[test]
fn save_as_new_path() {
    // Start with a bare core (no named buffer) and type into it.
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    core.send_input(ch('i'));
    common::type_str(&core, SAVE_TEXT);
    core.send_input(key(Key::Esc));
    waiter.wait_for_flush(FLUSH_TIMEOUT);

    // SaveAs to a brand-new path that does not yet exist on disk.
    let (_dir, path) = common::temp_md_path();
    assert!(
        !path.exists(),
        "precondition: SaveAs target should not exist yet"
    );

    core.send_input(InputEvent::SaveAs { path: path.clone() });

    let contents = wait_for_file_contains(&path, SAVE_TEXT);
    assert!(
        contents.contains(SAVE_TEXT),
        "SaveAs target should contain the typed text; got {contents:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Index of the first non-whitespace character in `s`, if any.
fn first_non_space_col(s: &str) -> Option<usize> {
    s.char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map(|(i, _)| i)
}
