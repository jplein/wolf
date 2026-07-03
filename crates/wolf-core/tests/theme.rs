//! Tests for `wolf-core`'s WolfCursor / WolfTitlebar theme tracking: the
//! snapshot exposes the two optional app-level highlight groups via
//! `Snapshot.theme: ThemeColors` (`{ cursor: ThemeGroup, titlebar: ThemeGroup }`,
//! each channel an `Option<Rgb>`, all-`None` when a group is absent or cleared).
//!
//! The value is delivered by a `ColorScheme` autocmd installed by
//! `_G.wolf_setup_theme_tracking(chan)` that `rpcnotify`s `"wolf_theme"` with
//! four flat scalars (`cursor_fg, cursor_bg, titlebar_fg, titlebar_bg`, `-1` =
//! unset); the helper also emits once immediately so the startup colorscheme is
//! always reported. Because that notification is SEPARATE from redraw flushes
//! (it wakes but does not bump `seq`), these tests do not assume a single flush
//! carries it — they poll `read_snapshot().theme` with a generous timeout,
//! draining wakes as they go (the `current_file.rs` pattern).
//!
//! NOTE on `:doautocmd ColorScheme`: a plain `:hi` change does NOT fire the
//! `ColorScheme` autocmd (only `:colorscheme` / `:set background=` re-sourcing
//! do), so every `:hi` mutation below is followed by an explicit
//! `:doautocmd ColorScheme` to trigger the tracker — this is REQUIRED by design,
//! not a test convenience.
//!
//! All tests drive a REAL, hermetic nvim (no mocks) via `WOLF_NVIM_BIN`; the
//! common helpers panic loudly if it is unset. The plain repo `init.lua` is used
//! (no lighthouse staged), so no colorscheme defines the Wolf groups at startup.

mod common;

use std::time::{Duration, Instant};

use common::{ch, key, type_str, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{Key, Rgb, ThemeColors, ThemeGroup, WolfCore};

/// How long to poll for `Snapshot.theme` to reach an expected value before
/// failing. The `wolf_theme` notification arrives independently of redraw
/// flushes and nvim startup is not instantaneous, so we wait well beyond the
/// expected latency.
const THEME_TIMEOUT: Duration = FLUSH_TIMEOUT;

/// Gap between `read_snapshot` polls while waiting for the theme to settle.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// How long to let the initial emit-once settle before asserting that a core
/// with no Wolf groups defined reports an all-`None` theme. Generous so a late
/// emit can't sneak a stale `Some(..)` past the assertion. Mirrors
/// `current_file.rs`'s `UNNAMED_SETTLE`.
const STARTUP_SETTLE: Duration = Duration::from_secs(2);

// -- The runtime-`:hi` fixture colors (test 2 / test 4) -----------------------
/// `WolfCursor guifg` used by the runtime-change tests.
const CURSOR_FG: u32 = 0x0011_2233;
/// `WolfCursor guibg` used by the runtime-change tests.
const CURSOR_BG: u32 = 0x0044_5566;
/// `WolfTitlebar guibg` used by the runtime-change tests (fg left unset).
const TITLEBAR_BG: u32 = 0x000A_0B0C;

/// Types `:{cmd}<CR>` as keystrokes: ':' enters command-line mode, `cmd` is
/// typed literally, Enter executes it. `cmd` must NOT include the leading ':'.
fn send_ex(core: &WolfCore, cmd: &str) {
    core.send_input(ch(':'));
    type_str(core, cmd);
    core.send_input(key(Key::Enter));
}

/// Polls `core.read_snapshot().theme` until `pred` holds, or panics after
/// [`THEME_TIMEOUT`] with `desc` and the last observed value. Drains pending
/// wakes each iteration so the wake channel never backs up while we poll.
fn wait_for_theme<F>(core: &WolfCore, waiter: &FlushWaiter, desc: &str, pred: F)
where
    F: Fn(&ThemeColors) -> bool,
{
    let deadline = Instant::now() + THEME_TIMEOUT;
    loop {
        let theme = core.read_snapshot().theme;
        if pred(&theme) {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out after {THEME_TIMEOUT:?} waiting for theme: {desc}; \
                 last observed: {theme:?}"
            );
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// The expected `ThemeColors` after the runtime `:hi` fixture commands land:
/// `WolfCursor guifg=#112233 guibg=#445566`, `WolfTitlebar guibg=#0a0b0c`.
fn expected_runtime_theme() -> ThemeColors {
    ThemeColors {
        cursor: ThemeGroup {
            fg: Some(Rgb::from_packed(CURSOR_FG)),
            bg: Some(Rgb::from_packed(CURSOR_BG)),
        },
        titlebar: ThemeGroup {
            fg: None,
            bg: Some(Rgb::from_packed(TITLEBAR_BG)),
        },
    }
}

/// Defines the two Wolf groups via `:hi` and fires `ColorScheme` so the tracker
/// re-reads them (plain `:hi` does not fire the autocmd — see module docs).
fn define_runtime_groups(core: &WolfCore) {
    send_ex(core, "hi WolfCursor guifg=#112233 guibg=#445566");
    send_ex(core, "hi WolfTitlebar guibg=#0a0b0c");
    send_ex(core, "doautocmd ColorScheme");
}

/// A core started with the plain repo `init.lua` (no lighthouse, no Wolf groups
/// defined anywhere) must report an all-`None` theme after startup: the helper's
/// emit-once ARRIVES (it always emits) but parses both groups as unset.
#[test]
fn startup_theme_is_all_none_without_wolf_groups() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    // Give the initial emit time to arrive (it always fires once), draining
    // wakes, then assert it left the theme at the all-None default.
    let settle_deadline = Instant::now() + STARTUP_SETTLE;
    while Instant::now() < settle_deadline {
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }

    let theme = core.read_snapshot().theme;
    assert_eq!(
        theme,
        ThemeColors::default(),
        "with no WolfCursor/WolfTitlebar defined, the startup emit must report \
         an all-None theme; got {theme:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Defining the groups at runtime via `:hi` and firing `ColorScheme` updates
/// `Snapshot.theme`: cursor fg+bg both set, titlebar bg-only (fg stays `None`).
#[test]
fn runtime_hi_change_updates_theme_on_colorscheme_autocmd() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    define_runtime_groups(&core);

    let want = expected_runtime_theme();
    wait_for_theme(
        &core,
        &waiter,
        "runtime :hi groups (cursor #112233/#445566, titlebar bg #0a0b0c)",
        |theme| *theme == want,
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A LINKED group resolves to the target's effective attributes: after
/// `:hi link WolfCursor Visual`, the tracker (reading with `link = false`)
/// reports Visual's effective bg — nvim's default colorscheme gives `Visual` a
/// background, so `theme.cursor.bg` must be `Some(..)`, not the link itself
/// (which carries no colors of its own).
#[test]
fn linked_wolf_cursor_resolves_to_effective_attributes() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    send_ex(&core, "hi link WolfCursor Visual");
    // Plain `:hi link` does not fire ColorScheme either — fire it explicitly.
    send_ex(&core, "doautocmd ColorScheme");

    wait_for_theme(
        &core,
        &waiter,
        "linked WolfCursor -> Visual to resolve to an effective bg",
        |theme| theme.cursor.bg.is_some(),
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Clearing the groups falls back to the all-`None` theme (the documented
/// fallback: cursor reverts to reverse-video logic, titlebar to system color).
/// First defines both groups and waits for them, so the clear is observed as a
/// genuine Some -> None transition rather than a never-set default.
#[test]
fn cleared_groups_fall_back_to_all_none() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    // Arrange: theme the groups and wait until the snapshot reflects them.
    define_runtime_groups(&core);
    let want = expected_runtime_theme();
    wait_for_theme(&core, &waiter, "groups defined before clearing", |theme| {
        *theme == want
    });

    // Act: clear both groups. `:hi clear <group>` does not fire ColorScheme, so
    // fire it explicitly (required by design — see module docs).
    send_ex(&core, "hi clear WolfCursor");
    send_ex(&core, "hi clear WolfTitlebar");
    send_ex(&core, "doautocmd ColorScheme");

    // Assert: back to the all-None default.
    wait_for_theme(
        &core,
        &waiter,
        "cleared groups to report an all-None theme again",
        |theme| *theme == ThemeColors::default(),
    );

    core.shutdown().expect("shutdown should be Ok");
}
