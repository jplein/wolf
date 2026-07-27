//! Integration test for the ANONYMOUS-BUFFER naming feature (plan §"Name anonymous
//! buffers `Untitled.md`"): every anonymous/new editable buffer gets a real, unsaved
//! markdown name — `Untitled.md`, then `Untitled 2.md`, `Untitled 3.md`, … (index 1
//! is un-numbered, so `Untitled 1.md` is NEVER generated) — choosing the first name
//! that collides with neither an existing Neovim buffer NOR a file on disk in cwd.
//! Naming sets `filetype=markdown` (so the in-app preview gate passes) but NEVER
//! writes the file to disk: an `Untitled N.md` buffer is a new, unsaved document
//! until the user explicitly `:w`-saves it.
//!
//! This drives the RUST half against a REAL, hermetic Neovim — NO mocks, per the
//! project testing policy. It is the sibling of `file_actions.rs` / `preview.rs`
//! (same staging / SKIP-when-unset machinery, reusing `tests/common/mod.rs`, and the
//! same "poll `read_frame()` until attached" pattern).
//!
//! ## Coverage (the plan's Test plan enumeration)
//!
//! 1. `startup_names_untitled_markdown_no_auto_write` — startup naming + no auto-write.
//! 2. `new_buffer_collides_up_the_sequence` — Cmd-N buffer collision: Untitled.md ->
//!    Untitled 2.md -> Untitled 3.md.
//! 3. `on_disk_collision_skips_to_untitled_2` — a real `Untitled.md` on disk forces
//!    the startup buffer to `Untitled 2.md`.
//! 4. `first_name_is_unnumbered_untitled` — index 1 is the bare `Untitled.md`, never
//!    `Untitled 1.md`.
//! 5. `new_buffer_writes_nothing_until_explicit_save` — naming never writes; an
//!    explicit `:w` DOES produce the file.
//! 6. `chdir_rehomes_anonymous_buffer` — opening a folder re-homes the untouched
//!    anonymous buffer into the new cwd (via the `Chdir` guard).
//! 7. `chdir_leaves_real_file_untouched` — the guard leaves a real, named file put
//!    across a `chdir` (no re-home).
//!
//! ## Dependency on coder work
//!
//! Backed by the `wolf_new_untitled` / `wolf_name_untitled_current` Lua helpers +
//! the `VimEnter` `WolfUntitled` autocmd in `runtime/nvim/init.lua`, wired into the
//! `NewBuffer` and `Chdir` arms of `crates/wolf-core/src/input.rs`. No new FFI method
//! is added — this exercises the existing `new_buffer` / `chdir` / `open` /
//! `current_file` / `is_markdown_active` surface. This test file does NOT touch BUILD
//! files (the coder adds the `//crates/wolf-ffi:untitled` `rust_test` target).
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim located via `WOLF_NVIM_BIN` (or
//! the Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is available,
//! via `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.

mod common;

use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent};

/// Generous overall timeout for nvim `--embed` startup plus each async effect (the
/// naming notification round-trip) to settle. Mirrors the sibling tests.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for an effect to surface. Mirrors the siblings.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// The first (un-numbered) anonymous name; index 1 is implicit.
const UNTITLED_1: &str = "Untitled.md";
/// The second anonymous name (index 2 IS numbered).
const UNTITLED_2: &str = "Untitled 2.md";
/// The third anonymous name.
const UNTITLED_3: &str = "Untitled 3.md";
/// The name that must NEVER be generated (index 1 is un-numbered).
const UNTITLED_1_NUMBERED: &str = "Untitled 1.md";

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

/// Starts a real nvim-backed core rooted at `cwd`, mirroring the sibling tests'
/// argument order exactly: (init_lua, cwd, cols, rows).
fn start_core(cwd: &Path) -> wolf_ffi::WolfCoreHandle {
    let init_lua = common::init_lua_path();
    wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        cwd.to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)")
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

/// Blocks (bounded by [`FLUSH_TIMEOUT`]) until the core's `current_file()` ends with
/// `suffix` (a BASENAME match, robust to absolute-vs-cwd-relative display and to
/// `/var`<->`/private/var` symlinking). Panics with `suffix` on timeout. Mirrors
/// `preview.rs::wait_for_current_file`.
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
         {suffix:?} (got {current:?})"
    );
}

/// Blocks until `handle.is_markdown_active()` is true, then asserts it. Mirrors
/// `preview.rs::wait_for_markdown_active` (specialized to the true case).
fn wait_for_markdown_active(handle: &wolf_ffi::WolfCoreHandle, desc: &str) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut got = handle.is_markdown_active();
    while Instant::now() < deadline {
        got = handle.is_markdown_active();
        if got {
            return;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(got, "is_markdown_active() should be true ({desc})");
}

/// Blocks until `handle.cwd()` canonicalizes to `target`, then asserts it. Mirrors
/// the `file_actions.rs` canonicalize-compare (robust to `/var`<->`/private/var`).
fn wait_for_cwd(handle: &wolf_ffi::WolfCoreHandle, target: &Path) {
    let canonical_target = target
        .canonicalize()
        .expect("failed to canonicalize the chdir target");
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut reported = handle.cwd();
    while Instant::now() < deadline {
        reported = handle.cwd();
        if let Ok(canonical_reported) = Path::new(&reported).canonicalize() {
            if canonical_reported == canonical_target {
                return;
            }
        }
        sleep(POLL_INTERVAL);
    }
    let canonical_reported = Path::new(&reported)
        .canonicalize()
        .unwrap_or_else(|_| Path::new(&reported).to_path_buf());
    assert_eq!(
        canonical_reported, canonical_target,
        "cwd() should follow chdir (reported={reported:?}, target={canonical_target:?})"
    );
}

/// The basename (final path component) of `path`, as an owned `String`.
fn basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[test]
fn startup_names_untitled_markdown_no_auto_write() {
    // Case 1: a fresh, empty cwd. The startup buffer must be the anonymous
    // `Untitled.md` — a real, markdown-typed name — yet NOTHING is written to disk
    // (no auto-save).
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    // Capture the cwd path BEFORE start so the disk assertion is unambiguous. Kept
    // alive until after `shutdown()`.
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    let cwd = tmp.path().to_path_buf();

    let handle = start_core(&cwd);
    wait_for_attach(&handle);

    // The startup buffer is named Untitled.md and is markdown-typed.
    wait_for_current_file(&handle, UNTITLED_1);
    wait_for_markdown_active(&handle, "startup anonymous Untitled.md buffer");

    // ...but naming did NOT write the file to disk.
    assert!(
        !cwd.join(UNTITLED_1).exists(),
        "naming the anonymous buffer must NOT auto-write {UNTITLED_1} to disk"
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn new_buffer_collides_up_the_sequence() {
    // Case 2: with the startup buffer already holding `Untitled.md`, each Cmd-N
    // (`new_buffer`) must pick the next free name: Untitled 2.md, then Untitled 3.md
    // (buffer-name collision, not disk collision).
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    let cwd = tmp.path().to_path_buf();

    let handle = start_core(&cwd);
    wait_for_attach(&handle);

    // Startup buffer occupies Untitled.md.
    wait_for_current_file(&handle, UNTITLED_1);

    // First Cmd-N: Untitled.md is taken by the (still-listed) startup buffer, so the
    // fresh buffer becomes Untitled 2.md.
    handle.new_buffer();
    wait_for_current_file(&handle, UNTITLED_2);

    // Second Cmd-N: Untitled.md and Untitled 2.md are both taken -> Untitled 3.md.
    handle.new_buffer();
    wait_for_current_file(&handle, UNTITLED_3);

    handle.shutdown();
    drop(tmp);
}

#[test]
fn on_disk_collision_skips_to_untitled_2() {
    // Case 3: a real `Untitled.md` file already on disk in cwd forces the startup
    // buffer to skip to `Untitled 2.md` (disk collision), still markdown-typed.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    let cwd = tmp.path().to_path_buf();

    // Seed a real Untitled.md on disk BEFORE starting the core.
    std::fs::write(cwd.join(UNTITLED_1), "# pre-existing\n")
        .expect("failed to seed the on-disk Untitled.md");

    let handle = start_core(&cwd);
    wait_for_attach(&handle);

    // The on-disk Untitled.md is a collision, so the anonymous buffer is Untitled 2.md.
    wait_for_current_file(&handle, UNTITLED_2);
    wait_for_markdown_active(&handle, "startup Untitled 2.md buffer (disk collision)");

    handle.shutdown();
    drop(tmp);
}

#[test]
fn first_name_is_unnumbered_untitled() {
    // Case 4: the first index is the bare `Untitled.md` — the literal
    // `Untitled 1.md` must NEVER be generated.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    let cwd = tmp.path().to_path_buf();

    let handle = start_core(&cwd);
    wait_for_attach(&handle);

    wait_for_current_file(&handle, UNTITLED_1);

    // Exact-basename check: it is `Untitled.md`, not `Untitled 1.md`.
    let current = handle.current_file();
    let base = basename(&current);
    assert_eq!(
        base, UNTITLED_1,
        "the first anonymous name must be the un-numbered {UNTITLED_1:?}, got {base:?}"
    );
    assert_ne!(
        base, UNTITLED_1_NUMBERED,
        "the literal {UNTITLED_1_NUMBERED:?} must NEVER be generated"
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn new_buffer_writes_nothing_until_explicit_save() {
    // Case 5: naming (startup + Cmd-N) never writes to disk; only an explicit `:w`
    // produces the file.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    let cwd = tmp.path().to_path_buf();

    let handle = start_core(&cwd);
    wait_for_attach(&handle);
    wait_for_current_file(&handle, UNTITLED_1);

    // Cmd-N -> Untitled 2.md.
    handle.new_buffer();
    wait_for_current_file(&handle, UNTITLED_2);

    // Neither the startup name nor the new name has been written to disk.
    assert!(
        !cwd.join(UNTITLED_1).exists(),
        "naming must NOT auto-write {UNTITLED_1} to disk"
    );
    assert!(
        !cwd.join(UNTITLED_2).exists(),
        "naming must NOT auto-write {UNTITLED_2} to disk"
    );

    // An explicit `:w` DOES write the current (Untitled 2.md) buffer — proving the
    // buffer is a normal, `:w`-able document.
    send_ex_command(&handle, ":w");
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut saved = cwd.join(UNTITLED_2).exists();
    while Instant::now() < deadline {
        saved = cwd.join(UNTITLED_2).exists();
        if saved {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        saved,
        "an explicit :w must write {UNTITLED_2} to disk (the buffer is :w-able)"
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn chdir_rehomes_anonymous_buffer() {
    // Case 6: opening a folder (a `chdir`) re-homes the untouched anonymous buffer
    // into the NEW cwd, so its name follows the working directory. Still no file is
    // written in the new folder.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    // Two fresh, empty dirs: A (launch cwd) and B (the "opened folder").
    let dir_a = tempfile::tempdir().expect("failed to create temp cwd A");
    let dir_b = tempfile::tempdir().expect("failed to create temp cwd B");
    let cwd_a = dir_a.path().to_path_buf();
    let cwd_b = dir_b.path().to_path_buf();
    let canonical_b = cwd_b
        .canonicalize()
        .expect("failed to canonicalize dir B");

    let handle = start_core(&cwd_a);
    wait_for_attach(&handle);

    // Startup buffer is A/Untitled.md.
    wait_for_current_file(&handle, UNTITLED_1);

    // Open folder B: chdir re-homes the anonymous buffer into B.
    handle.chdir(cwd_b.to_string_lossy().into_owned());
    wait_for_cwd(&handle, &cwd_b);

    // Poll until current_file() is Untitled.md WHOSE PARENT is B (the re-home
    // landed). `ends_with` basename + canonicalized-parent compare, robust to
    // `/var`<->`/private/var`.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut current = handle.current_file();
    let mut rehomed = false;
    while Instant::now() < deadline {
        current = handle.current_file();
        if current.ends_with(UNTITLED_1) {
            if let Some(parent) = Path::new(&current).parent() {
                if let Ok(canonical_parent) = parent.canonicalize() {
                    if canonical_parent == canonical_b {
                        rehomed = true;
                        break;
                    }
                }
            }
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        rehomed,
        "chdir must re-home the anonymous buffer to <B>/{UNTITLED_1} (got {current:?}, B={canonical_b:?})"
    );

    // The re-home did NOT write a file into B.
    assert!(
        !cwd_b.join(UNTITLED_1).exists(),
        "re-homing must NOT auto-write {UNTITLED_1} into the opened folder"
    );

    handle.shutdown();
    drop(dir_a);
    drop(dir_b);
}

#[test]
fn chdir_leaves_real_file_untouched() {
    // Case 7: the re-home guard is a no-op for a REAL, named file. Opening a real
    // markdown file then chdir-ing elsewhere must leave the current buffer pointed
    // at that real file (no re-home to Untitled.md).
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    const REAL_FILE: &str = "real.md";

    let dir_a = tempfile::tempdir().expect("failed to create temp cwd A");
    let dir_b = tempfile::tempdir().expect("failed to create temp cwd B");
    let cwd_a = dir_a.path().to_path_buf();
    let cwd_b = dir_b.path().to_path_buf();

    // Seed a real markdown file in A and open it.
    let real_path = cwd_a.join(REAL_FILE);
    std::fs::write(&real_path, "# Real Heading\n\nBody.\n").expect("failed to seed real.md");

    let handle = start_core(&cwd_a);
    wait_for_attach(&handle);

    handle.open(real_path.to_string_lossy().into_owned());
    wait_for_current_file(&handle, REAL_FILE);

    // chdir to B: the guard (name is a real, non-Untitled file) prevents re-home.
    handle.chdir(cwd_b.to_string_lossy().into_owned());
    wait_for_cwd(&handle, &cwd_b);

    // After the chdir landed, the current buffer is STILL the real file — the guard
    // left it untouched (it was NOT renamed to Untitled.md).
    let current = handle.current_file();
    assert!(
        current.ends_with(REAL_FILE),
        "the guard must leave a real named file put across a chdir (got {current:?})"
    );
    assert!(
        !current.ends_with(UNTITLED_1),
        "the guard must NOT re-home a real file to {UNTITLED_1} (got {current:?})"
    );

    handle.shutdown();
    drop(dir_a);
    drop(dir_b);
}
