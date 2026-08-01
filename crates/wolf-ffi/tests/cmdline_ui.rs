//! Integration test for the `wolf-ffi` NATIVE command-line / completion-popup /
//! message-toast bridge surface (feature plan §5).
//!
//! This drives the RUST half of the swift-bridge cmdline machinery against a REAL,
//! hermetic Neovim — NO mocks, per the project testing policy. It is the sibling
//! of `panes.rs` / `theme.rs` (same staging / SKIP-when-unset machinery, reusing
//! `tests/common/mod.rs`) but exercises the NEW cmdline / pum / toast accessors:
//!
//!   * `WolfCoreHandle::cmdline() -> FfiCmdline` (`active()`, `firstc()`, `text()`,
//!     `pos()`).
//!   * `WolfCoreHandle::pum() -> FfiPum` (`visible()`, `selected()`, `items()` ->
//!     `Vec<FfiPumItem>` with `word()` / `kind()` / `menu()` / `info()`).
//!   * `WolfCoreHandle::toast() -> FfiToast` (`visible()`, `kind()`, `lines()` ->
//!     `Vec<FfiToastLine>` -> `chunks()` -> `Vec<FfiToastChunk>` with `text()` and
//!     the color accessors `fg()` / `bg()` / `special()`, RESOLVED in Rust against
//!     the highlight table like `FfiCell`).
//!
//! ## Dependency on coder work (plan §5)
//!
//! These accessors + opaque types do not exist yet: the coder implements them in
//! `crates/wolf-ffi/src/lib.rs`, regenerates `generated/`, AND adds the
//! `//crates/wolf-ffi:cmdline_ui` `rust_test` target to `BUILD.bazel` (mirroring
//! `theme`). This test file does NOT touch BUILD files.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings, this test needs a pinned nvim located via `WOLF_NVIM_BIN`
//! (or the Bazel-staged tree). It **loudly SKIPS** (no-op pass) when none is
//! available, via `common::nvim_bin_or_skip`. See `tests/common/mod.rs`.

mod common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiColor, FfiKeyEvent, WolfCoreHandle};

/// Generous overall timeout for nvim `--embed` startup plus each cmdline / pum /
/// toast change to settle. Mirrors the sibling tests' `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for attach / a cmdline change to surface. Mirrors
/// the sibling tests' `POLL_INTERVAL`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;
/// The Esc key (aborts the command line).
const KIND_ESC: u8 = 2;
/// The Tab key (triggers cmdline completion).
const KIND_TAB: u8 = 3;
/// The Left arrow (a harmless movement key used to dismiss the toast).
const KIND_LEFT: u8 = 6;

/// The command-line-mode prefix Neovim reports for an Ex command (`:`).
const EX_FIRSTC: &str = ":";
/// The cmdline text after `e Fi` is typed (the `:` is the firstc, not content).
const CMDLINE_TEXT: &str = "e Fi";

/// The two `Fi`-prefixed files that Neovim's prefix completion must offer.
const FILE_A: &str = "File A.md";
const FILE_B: &str = "File B.md";
/// A third file that does NOT share the `Fi` prefix.
const FILE_OTHER: &str = "Something else.md";
/// The minimum number of popupmenu items expected for the two-way `Fi` completion.
const MIN_PUM_ITEMS: usize = 2;

/// The message `kind` requested for the multi-line `nvim_echo`.
const LIST_CMD_KIND: &str = "list_cmd";

/// An invalid Ex command name. Executing `:{this}` yields an `E492: Not an editor
/// command` error, an `emsg`-kind message whose content chunk carries the
/// `ErrorMsg` highlight — so its resolved fg differs from an unstyled echo.
const BOGUS_EX_COMMAND: &str = "Zzzboguscommand";

/// Builds a printable-character `FfiKeyEvent` (kind=Char) for `c`, all modifiers
/// false (the already-resolved glyph rides in `ch`, per the Shift contract).
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
/// generated Swift glue uses. Does NOT press Enter (callers that want to observe
/// the live command line must not execute it).
fn type_str(handle: &WolfCoreHandle, s: &str) {
    for c in s.chars() {
        handle.send_key(char_key(c));
    }
}

/// Types `cmd` as an Ex command and presses Enter. `cmd` must include the leading
/// `:`. Mirrors `panes.rs`.
fn send_ex_command(handle: &WolfCoreHandle, cmd: &str) {
    type_str(handle, cmd);
    handle.send_key(plain_key(KIND_ENTER));
}

/// Strips backslash escapes from a completion word so a space-containing filename
/// compares the same whether Neovim reports it raw or shell-escaped.
fn deescape(s: &str) -> String {
    s.chars().filter(|&c| c != '\\').collect()
}

/// The flattened toast content: each line's chunk texts concatenated, lines joined
/// with `\n`. Used for substring assertions across the bridge.
fn toast_text(handle: &WolfCoreHandle) -> String {
    handle
        .toast()
        .lines()
        .iter()
        .map(|line| line.chunks().iter().map(|c| c.text()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Polls until the toast is visible and its flattened text contains `needle`,
/// then returns the resolved fg of the first chunk of the first line. Panics on
/// timeout or if the surfaced toast is missing a first chunk.
fn poll_first_chunk_fg(handle: &WolfCoreHandle, needle: &str) -> FfiColor {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        let toast = handle.toast();
        if toast.visible() && toast_text(handle).contains(needle) {
            let lines = toast.lines();
            let first_line = lines
                .first()
                .expect("the toast should have at least one line");
            let chunks = first_line.chunks();
            let first_chunk = chunks
                .first()
                .expect("the first toast line should have a chunk");
            return first_chunk.fg();
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for a toast whose text contains {needle:?}"
        );
        sleep(POLL_INTERVAL);
    }
}

/// OD1: the toast keeps Neovim's STYLED color resolved in Rust, not a flattened
/// monochrome. An error message (`emsg`, `ErrorMsg`-highlighted) must resolve a
/// chunk fg DISTINCT from an unstyled `list_cmd` echo's chunk fg.
///
/// The comparison is tolerant of theme-specific RGB (it asserts "differs from the
/// plain-echo chunk color" rather than a hardcoded value). It is deterministic
/// against the pinned nvim's default colorscheme, where `ErrorMsg` is red and the
/// unstyled echo resolves to the default foreground. If a future pinned scheme
/// made `ErrorMsg` equal the default fg, this strict distinctness would need
/// softening — see the test-writer report.
#[test]
fn emsg_toast_keeps_styled_color() {
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");

    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // Poll read_frame() until attached (cols > 0).
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        if handle.read_frame().cols() > 0 {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        handle.read_frame().cols() > 0,
        "core should attach and report a non-empty active frame (cols)"
    );

    // 1. A plain, unstyled `list_cmd` echo -> capture its first-chunk fg. Typing
    //    `:` for this command is a dismissing key that clears any prior toast.
    send_ex_command(
        &handle,
        r#":lua vim.api.nvim_echo({{"plain echo text"}}, false, { kind = "list_cmd" })"#,
    );
    let plain_fg = poll_first_chunk_fg(&handle, "plain echo text");

    // 2. An error message: `:Zzzboguscommand` -> `E492` `emsg`, `ErrorMsg`-styled.
    //    Typing `:` first clears the plain toast; the error then repopulates it.
    send_ex_command(&handle, &format!(":{BOGUS_EX_COMMAND}"));
    let err_fg = poll_first_chunk_fg(&handle, "E492");

    // The error chunk's fg is resolved (set) in Rust and DISTINCT from the plain
    // echo's fg — the toast preserves Neovim's per-message styled color (OD1).
    assert!(
        err_fg.set,
        "the emsg chunk fg should be a resolved (set) color (rgb {:#08x})",
        err_fg.rgb
    );
    assert!(
        err_fg.set != plain_fg.set || err_fg.rgb != plain_fg.rgb,
        "an error message's fg (rgb {:#08x}, set {}) must differ from a plain echo's \
         fg (rgb {:#08x}, set {}) — the toast keeps Neovim's styled color (OD1), \
         not one flattened foreground",
        err_fg.rgb,
        err_fg.set,
        plain_fg.rgb,
        plain_fg.set
    );

    handle.shutdown();
    drop(tmp);
}

#[test]
fn cmdline_pum_and_toast_over_the_bridge() {
    // Loud SKIP (no-op pass) when no pinned nvim is available, exactly like the
    // sibling tests. Otherwise export the resolved binary so wolf-core launches it.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let init_lua = common::init_lua_path();

    // A temp cwd seeded with the three files so `:e Fi` completes against it. It is
    // nvim's working dir + search root and must outlive the session, so keep the
    // guard alive until after `shutdown()`.
    let tmp = tempfile::tempdir().expect("failed to create temp cwd directory");
    for name in [FILE_A, FILE_B, FILE_OTHER] {
        std::fs::write(tmp.path().join(name), "x\n").expect("failed to write seed file");
    }

    // Argument order mirrors the sibling tests exactly: (init_lua, cwd, cols, rows).
    let handle = wolf_ffi_start(
        init_lua.to_string_lossy().into_owned(),
        tmp.path().to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- 1. Poll read_frame() until attached (cols > 0) ---------------------
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

    // --- 2. `:` then `e Fi` -> handle.cmdline() active with the typed text ---
    handle.send_key(char_key(':'));
    type_str(&handle, CMDLINE_TEXT);

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        let cm = handle.cmdline();
        if cm.active() && cm.firstc() == EX_FIRSTC && cm.text() == CMDLINE_TEXT {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "cmdline() never reported active firstc `:` text `e Fi` (active={}, firstc={:?}, text={:?})",
            cm.active(),
            cm.firstc(),
            cm.text()
        );
        sleep(POLL_INTERVAL);
    }
    let cm = handle.cmdline();
    assert!(
        cm.active(),
        "cmdline() should be active while typing `:e Fi`"
    );
    assert_eq!(
        cm.firstc(),
        EX_FIRSTC,
        "cmdline() firstc should be the Ex `:` prefix"
    );
    assert_eq!(cm.text(), CMDLINE_TEXT, "cmdline() text should be `e Fi`");
    assert!(
        cm.pos() > 0,
        "cmdline() pos should have advanced past 0 after typing `e Fi` (got {})",
        cm.pos()
    );

    // --- 3. <Tab> -> handle.pum() visible with the two `Fi` files -----------
    handle.send_key(plain_key(KIND_TAB));

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        let pum = handle.pum();
        if pum.visible() && pum.items().len() >= MIN_PUM_ITEMS {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "pum() never became visible with >= {MIN_PUM_ITEMS} items after <Tab> \
             (visible={}, items={})",
            pum.visible(),
            pum.items().len()
        );
        sleep(POLL_INTERVAL);
    }
    let pum = handle.pum();
    assert!(pum.visible(), "pum() should be visible after <Tab>");
    assert!(
        pum.items().len() >= MIN_PUM_ITEMS,
        "the two-way `Fi` completion should offer at least {MIN_PUM_ITEMS} items; got {}",
        pum.items().len()
    );
    assert!(
        pum.selected() >= 0,
        "pum() selected() should point at the first match after <Tab>; got {}",
        pum.selected()
    );
    let words: Vec<String> = pum.items().iter().map(|it| deescape(&it.word())).collect();
    assert!(
        words.iter().any(|w| w.contains(FILE_A)),
        "the pum items should include `{FILE_A}` (deescaped); got {words:?}"
    );
    assert!(
        words.iter().any(|w| w.contains(FILE_B)),
        "the pum items should include `{FILE_B}` (deescaped); got {words:?}"
    );
    assert!(
        !words.iter().any(|w| w.contains(FILE_OTHER)),
        "`{FILE_OTHER}` does not share the `Fi` prefix and must NOT be a candidate; got {words:?}"
    );

    // --- 4. <Esc> -> cmdline inactive AND pum hidden ------------------------
    handle.send_key(plain_key(KIND_ESC));
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        if !handle.cmdline().active() && !handle.pum().visible() {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        !handle.cmdline().active(),
        "cmdline() should be inactive after <Esc>"
    );
    assert!(
        !handle.pum().visible(),
        "pum() should be hidden after <Esc>"
    );

    // --- 5. Drive a multi-line echo -> handle.toast() -----------------------
    send_ex_command(
        &handle,
        r#":lua vim.api.nvim_echo({{"line one\n"},{"line two"}}, false, { kind = "list_cmd" })"#,
    );

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        let toast = handle.toast();
        if toast.visible() {
            let text = toast_text(&handle);
            if text.contains("line one") && text.contains("line two") {
                break;
            }
        }
        sleep(POLL_INTERVAL);
    }
    let toast = handle.toast();
    assert!(toast.visible(), "toast() should be visible after the echo");
    assert_eq!(
        toast.kind(),
        LIST_CMD_KIND,
        "toast() kind() should reflect the requested `{LIST_CMD_KIND}` kind"
    );
    let text = toast_text(&handle);
    assert!(
        text.contains("line one") && text.contains("line two"),
        "toast() should carry BOTH echoed lines; got {text:?}"
    );

    // Inspect the first chunk of the first line: its text is non-empty and its
    // foreground color is resolved in Rust (set) against the highlight table.
    let lines = toast.lines();
    let first_line = lines
        .first()
        .expect("the toast should have at least one line");
    let chunks = first_line.chunks();
    let first_chunk = chunks
        .first()
        .expect("the first toast line should have a chunk");
    assert!(
        !first_chunk.text().is_empty(),
        "the first toast chunk should carry non-empty text"
    );
    assert!(
        first_chunk.fg().set,
        "the first toast chunk's fg() should be a resolved (set) color, not a fallback \
         (rgb {:#08x})",
        first_chunk.fg().rgb
    );

    // --- 6. A movement key clears the toast across the bridge ---------------
    handle.send_key(plain_key(KIND_LEFT));
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while Instant::now() < deadline {
        if !handle.toast().visible() {
            break;
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        !handle.toast().visible(),
        "toast() should clear after a movement key (dismiss-on-input across the bridge)"
    );

    // --- 7. Shutdown: clean teardown ----------------------------------------
    handle.shutdown();
    drop(tmp);
}
