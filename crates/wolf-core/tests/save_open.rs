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
            let dump =
                std::fs::read_to_string(path).unwrap_or_else(|e| format!("<unreadable: {e}>"));
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
    let grid = snap.active_grid();

    let row0 = grid.row_text(0);
    assert!(
        row0.contains("# Title"),
        "row 0 should contain the heading text; got {row0:?}"
    );

    // Basic syntax-highlight smoke test: the heading row should carry a different
    // hl_id from a plain body cell. Markdown highlighting is enabled in init.lua.
    // Pick the first non-space cell on the heading row (row 0) and on the body
    // row (row 1) and assert their hl_ids differ.
    let heading_col = first_non_space_col(&row0).expect("heading row should have a non-space char");
    let heading_cell = grid.cell(0, heading_col).expect("expected a heading cell");

    let row1 = grid.row_text(1);
    let body_col = first_non_space_col(&row1).expect("body row should have a non-space char");
    let body_cell = grid.cell(1, body_col).expect("expected a body cell");

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
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

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
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    // SaveAs to a brand-new path that does not yet exist on disk.
    let (_dir, path) = common::temp_md_path();
    assert!(
        !path.exists(),
        "precondition: SaveAs target should not exist yet"
    );

    core.send_input(InputEvent::SaveAs {
        path: path.clone(),
        force: false,
    });

    let contents = wait_for_file_contains(&path, SAVE_TEXT);
    assert!(
        contents.contains(SAVE_TEXT),
        "SaveAs target should contain the typed text; got {contents:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Text seeded into the file a `SaveAs` is aimed at, so the overwrite pair can tell
/// "the file was left alone" from "the file was replaced".
///
/// Deliberately unlike [`SAVE_TEXT`]: the whole discrimination in both tests below is
/// which of the two strings the file holds afterwards, so a needle that appeared in
/// both would make the negative test pass on the positive test's outcome.
const ORIGINAL_TEXT: &str = "original untouched line";

/// How long to let a non-forcing `SaveAs` fail before reading the target back.
///
/// A NEGATIVE assertion needs a settle rather than a poll: there is no file event to
/// wait for, so the only honest way to say "it was not overwritten" is to give the
/// write every chance to land first. Sized generously against the positive test's
/// measured latency — [`save_as_with_force_overwrites`] polls the same round trip and
/// its poll budget is [`SAVE_POLL_TIMEOUT`] — so a green here is not a race won.
const REFUSAL_SETTLE: Duration = Duration::from_secs(3);

/// Polls `read` until it answers `Some`, or panics after [`SAVE_POLL_TIMEOUT`].
///
/// Used for `current_file`, which arrives out of band on the `wolf_current_file`
/// notification and does NOT bump `seq`, so `wait_for_flush` cannot be used for it.
fn wait_for<T>(what: &str, read: impl Fn() -> Option<T>) -> T {
    let deadline = std::time::Instant::now() + SAVE_POLL_TIMEOUT;
    loop {
        if let Some(value) = read() {
            return value;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out after {SAVE_POLL_TIMEOUT:?} waiting for {what}"
        );
        std::thread::sleep(SAVE_POLL_INTERVAL);
    }
}

/// The path `Snapshot::current_file` will carry once `path` becomes the buffer's name.
///
/// `redraw::apply_current_file` CANONICALIZES every path it stores, while `tempfile`
/// builds its paths out of `TMPDIR` verbatim. On a host whose `TMPDIR` resolves through
/// a symlink the two spellings name one file and never compare equal, so a poll against
/// the raw path spins to its deadline and reports a rename that in fact happened. The
/// fallback matches core's own: a path that cannot be canonicalized is used as it is.
fn as_core_spells_it(path: &std::path::Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Polls `Snapshot::current_file` until it is `want`, and returns the LAST value it
/// read whether or not that ever happened.
///
/// Returning rather than panicking on the deadline is the point: the caller's own
/// `assert_eq!` then names both the value it wanted and the value it actually saw.
/// [`wait_for`]'s timeout panic can say only what it was waiting for, which on this
/// particular wait sends the reader to look at the rename path when the difference may
/// be nothing but a path spelling — see [`as_core_spells_it`].
fn wait_for_current_file(
    core: &wolf_core::WolfCore,
    want: &std::path::Path,
) -> Option<std::path::PathBuf> {
    let deadline = std::time::Instant::now() + SAVE_POLL_TIMEOUT;
    loop {
        let current = core.read_snapshot().current_file.clone();
        if current.as_deref() == Some(want) || std::time::Instant::now() >= deadline {
            return current;
        }
        std::thread::sleep(SAVE_POLL_INTERVAL);
    }
}

/// A live core whose focused buffer holds [`SAVE_TEXT`], plus an EXISTING file on disk
/// holding [`ORIGINAL_TEXT`] for a `SaveAs` to be aimed at.
///
/// The temp guards are held here and never read: dropping either would delete the file
/// out from under the assertions.
struct OverwriteFixture {
    core: wolf_core::WolfCore,
    /// The existing file a `SaveAs` is aimed at.
    target_path: std::path::PathBuf,
    /// The buffer's OWN file, kept so its path can be compared against the target.
    source: tempfile::NamedTempFile,
    _waiter: common::FlushWaiter,
    _target: tempfile::NamedTempFile,
}

/// Builds an [`OverwriteFixture`].
///
/// The two `SaveAs` tests below share this because the interesting difference between
/// them is one boolean, and a setup written twice would let them drift apart.
///
/// TWO files, not one: the buffer being saved must be a DIFFERENT buffer from the one
/// on disk, or `saveas` would be aimed at the buffer's own name and the `E13` arm
/// would never be reached at all.
fn start_core_over_an_existing_target() -> OverwriteFixture {
    let target = common::temp_md(ORIGINAL_TEXT);
    let target_path = target.path().to_path_buf();

    // The buffer that will be saved AS the target: its own separate file, opened
    // through the ordinary path so it carries a real name of its own.
    let (core, waiter, source) = common::start_core_with_temp_md("");

    core.send_input(ch('i'));
    common::type_str(&core, SAVE_TEXT);
    core.send_input(key(Key::Esc));
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    OverwriteFixture {
        core,
        target_path,
        source,
        _waiter: waiter,
        _target: target,
    }
}

/// A non-forcing `SaveAs` refuses to clobber an existing file, and leaves the buffer's
/// own name untouched.
///
/// ## What it discriminates
///
/// This is the arm the front end must NOT take, and the reason `force` exists at all:
/// plain `saveas` fails `E13: File exists`, and `input::apply_batch` degrades that to
/// a `tracing::warn!` nobody sees. Without a test, the failure is invisible from every
/// direction — no return value, no message, no changed file.
///
/// Two observables, and the pair is the point. The NEGATIVE half (the file still holds
/// [`ORIGINAL_TEXT`] and not [`SAVE_TEXT`]) could be satisfied by a write that merely
/// had not landed yet, so the POSITIVE half is what rules that out: `:saveas` fires
/// `BufFilePost`, which `runtime/nvim/init.lua`'s current-file autocmd emits on, so a
/// SUCCESSFUL rename would have moved `Snapshot::current_file` to the target. Reading
/// it back still on the source's own path says the rename did not happen, rather than
/// that it has not happened yet.
///
/// FALSIFYING EDIT: make `input::apply`'s `SaveAs` arm always emit `saveas!`. RED
/// here — the file comes back holding `SAVE_TEXT` and `current_file` has moved — while
/// [`save_as_with_force_overwrites`] stays GREEN. The two isolate each other.
#[test]
fn save_as_refuses_to_overwrite_without_force() {
    // Destructured rather than held as one value so `core.shutdown()` can consume the
    // core at the end; the two guard bindings keep both temp files alive until then.
    let OverwriteFixture {
        core,
        target_path,
        source,
        _waiter,
        _target,
    } = start_core_over_an_existing_target();

    // Precondition: the file really is on disk with the original text, and the buffer
    // really is a different one. Without both, the assertions below are about nothing.
    let before = std::fs::read_to_string(&target_path).expect("target file should exist on disk");
    assert!(
        before.contains(ORIGINAL_TEXT),
        "precondition: the SaveAs target should hold {ORIGINAL_TEXT:?} before the save; got \
         {before:?}"
    );
    assert!(
        !before.contains(SAVE_TEXT),
        "precondition: the SaveAs target must NOT already contain {SAVE_TEXT:?}, or the negative \
         assertion below would be satisfied by the fixture rather than by the refusal; got \
         {before:?}"
    );
    assert_ne!(
        target_path,
        source.path(),
        "precondition: the buffer being saved and the SaveAs target must be DIFFERENT files, or \
         `saveas` is aimed at the buffer's own name and the E13 arm is never reached"
    );
    let source_path = wait_for("the source buffer's current_file", || {
        core.read_snapshot().current_file.clone()
    });

    core.send_input(InputEvent::SaveAs {
        path: target_path.clone(),
        force: false,
    });

    // A settle, not a poll: there is no event that says "the write did not happen".
    // See REFUSAL_SETTLE for why this is not a race.
    std::thread::sleep(REFUSAL_SETTLE);

    let after = std::fs::read_to_string(&target_path).expect("target file should still exist");
    assert!(
        after.contains(ORIGINAL_TEXT),
        "a non-forcing SaveAs overwrote an existing file. `saveas` without `!` must fail `E13: \
         File exists` and leave the target alone — that failure is what a caller with its own \
         confirming dialog opts OUT of by passing force: true, and it is why the flag exists at \
         all. If this arm force-overwrites, every SaveAs in the API is destructive by default, \
         including from a front end that never asked the user. Target contents were {after:?}."
    );
    assert!(
        !after.contains(SAVE_TEXT),
        "a non-forcing SaveAs wrote the BUFFER's text into an existing file. Same finding as \
         above, seen from the other side: {SAVE_TEXT:?} exists only in the buffer, so its presence \
         on disk means the write landed. Target contents were {after:?}."
    );

    // The POSITIVE half, which is what keeps the two negatives from being a timing
    // artefact: `:saveas` renames the buffer and fires `BufFilePost`, so a successful
    // save would have moved `current_file` to the target.
    let current = core.read_snapshot().current_file.clone();
    assert_eq!(
        current.as_deref(),
        Some(source_path.as_path()),
        "after a REFUSED SaveAs the buffer's current_file is {current:?}, not the source path it \
         started on. `:saveas` renames the buffer and fires BufFilePost, which init.lua's \
         current-file autocmd emits on, so a moved current_file means the rename SUCCEEDED — which \
         makes the two assertions above a race this run happened to win rather than evidence of a \
         refusal. If current_file has moved to {} while the file on disk is unchanged, the finding \
         is in the rename path rather than in the write.",
        target_path.display()
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A forcing `SaveAs` overwrites an existing file, and renames the buffer to it.
///
/// The positive half of the pair, and the evidence for the whole `force` flag: it is
/// what the GTK front end requests, because its own dialog has already confirmed the
/// overwrite (`wolf_gnome::save_ui::save_as_target` pins the request side; only this
/// test can say the request has the effect it claims).
///
/// FALSIFYING EDIT: drop the `!` — i.e. make `input::apply`'s `SaveAs` arm always emit
/// plain `saveas`. RED here on the poll timeout, while
/// [`save_as_refuses_to_overwrite_without_force`] stays GREEN.
#[test]
fn save_as_with_force_overwrites() {
    let OverwriteFixture {
        core,
        target_path,
        source,
        _waiter,
        _target,
    } = start_core_over_an_existing_target();

    let before = std::fs::read_to_string(&target_path).expect("target file should exist on disk");
    assert!(
        before.contains(ORIGINAL_TEXT) && !before.contains(SAVE_TEXT),
        "precondition: the SaveAs target should hold {ORIGINAL_TEXT:?} and not {SAVE_TEXT:?} \
         before the save, or the poll below would be satisfied the instant it starts and this test \
         would prove nothing; got {before:?}"
    );
    assert_ne!(
        target_path,
        source.path(),
        "precondition: the buffer being saved and the SaveAs target must be DIFFERENT files, or \
         this test is not an overwrite at all"
    );

    core.send_input(InputEvent::SaveAs {
        path: target_path.clone(),
        force: true,
    });

    let contents = wait_for_file_contains(&target_path, SAVE_TEXT);
    assert!(
        contents.contains(SAVE_TEXT),
        "a forcing SaveAs should have replaced the target's contents with the buffer's; got \
         {contents:?}"
    );
    assert!(
        !contents.contains(ORIGINAL_TEXT),
        "a forcing SaveAs left {ORIGINAL_TEXT:?} in the target, so the buffer was APPENDED to the \
         existing file rather than written over it. `:saveas!` writes the whole buffer, so the \
         original line must be gone; contents were {contents:?}."
    );

    // The rename half. `:saveas` points the buffer at its new name, so every later
    // plain `write` goes to the target too — without this, a green above would be
    // consistent with a one-off write that left the buffer on its old file.
    //
    // Compared against the path AS CORE SPELLS IT, not against `tempfile`'s: see
    // `as_core_spells_it`. The poll returns whatever it last read, so the failure below
    // names the observed value rather than blaming a timeout on the rename.
    let expected = as_core_spells_it(&target_path);
    let current = wait_for_current_file(&core, &expected);
    assert_eq!(
        current.as_deref(),
        Some(expected.as_path()),
        "after a forcing SaveAs the buffer's current_file is {current:?}, not the target path \
         {expected:?} (canonicalized from {}). `:saveas!` RENAMES the buffer — that is what \
         distinguishes it from a plain write to another file — and fires BufFilePost, which \
         init.lua's current-file autocmd emits on. A current_file left on the old path means the \
         buffer's later saves would go somewhere the user did not choose, while this test's file \
         assertions above would still be green. If the two paths differ only in spelling, the \
         rename DID happen and the finding is in `as_core_spells_it`.",
        target_path.display()
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Index of the first non-whitespace character in `s`, if any.
fn first_non_space_col(s: &str) -> Option<usize> {
    s.char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map(|(i, _)| i)
}
