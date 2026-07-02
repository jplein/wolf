//! Regression test for the "occasionally cells draw with the wrong background"
//! bug: isolated cells inside an orlando `OrlandoCodeBlock` gray region
//! rendering with the default (black) background in the macOS app.
//!
//! ROOT CAUSE (fixed in `wolf-core/src/redraw.rs::grid_line`): Neovim emits a
//! zero-width trailing clear-to-EOL cell `[" ", hl 0, repeat 0]` at the end of
//! many `grid_line` events. The parser used to clamp `repeat` with `.max(1)`,
//! turning that no-op into a spurious one-cell write of a default-highlight
//! space at the column just past the event's real span. Neovim's shadow grid
//! never contains that cell, so it is never repainted — the wrong-background
//! cell persists on screen until unrelated churn happens to rewrite it. This
//! test reproduced the corruption deterministically (seed 0x5EED0001, iter 5,
//! after `dd`+`u` inside the second block) before the fix.
//!
//! This drives a REAL, hermetic Neovim (no mocks) with the vendored orlando
//! plugin staged exactly like `orlando_load.rs`, opens a Markdown document
//! containing fenced code blocks (mimicking the document in the bug report),
//! then hammers the session with the kinds of churn the report involved —
//! cursor motion, scrolling, in-block edits + undo, and fence insertion/removal
//! (which repaints everything below the fence twice) — and after EVERY op waits
//! for the frame to go idle and asserts two invariants over the visible window
//! grid:
//!
//!   1. TEXT INTEGRITY: every visible row's text equals the corresponding
//!      buffer line (the ops are all net-neutral, so the buffer is always back
//!      to the original document at check time). A violation means wolf-core's
//!      grid state diverged from Neovim's (misapplied `grid_line`/`grid_scroll`).
//!   2. BLOCK BACKGROUND UNIFORMITY: every cell of every visible row that lies
//!      inside a fenced code block (fences inclusive, `hl_eol` fill to the grid
//!      edge) has the SAME resolved background — the artifact in the bug report
//!      is exactly a violation of this.
//!
//! Because the checks run only at IDLE (stable frame, i.e. after the final
//! `flush` of the churn), a failure here proves PERSISTENT snapshot divergence
//! in wolf-core: Neovim's line-grid protocol only re-sends changed cells, so a
//! single misapplied event leaves stale cells forever.
//!
//! SKIP-when-unset policy: like `orlando_load`, loud no-op pass when the pinned
//! nvim or the staged orlando tree is absent. The lighthouse colorscheme tree is
//! staged too when available (the real bundle has it), but its absence does not
//! skip the test — the default colorscheme also gives `CursorLine` (the group
//! `OrlandoCodeBlock` links to) a distinct background.

mod common;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::RangeInclusive;
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent, WolfCoreHandle};

/// Generous overall timeout for nvim `--embed` startup / attach.
const ATTACH_TIMEOUT: Duration = Duration::from_secs(10);
/// Poll cadence while waiting for attach / idle frames.
const POLL_INTERVAL: Duration = Duration::from_millis(40);
/// A frame is "idle" when this many consecutive polls hash identically.
const IDLE_STREAK: usize = 3;
/// Give up waiting for idle after this long (the check then runs anyway).
const IDLE_TIMEOUT: Duration = Duration::from_secs(4);

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
const KIND_CHAR: u8 = 0;
const KIND_ENTER: u8 = 1;
const KIND_ESC: u8 = 2;

fn key(kind: u8, ch: u32, ctrl: bool) -> FfiKeyEvent {
    FfiKeyEvent {
        kind,
        ch,
        fnum: 0,
        ctrl,
        shift: false,
        alt: false,
        meta: false,
    }
}

fn send_chars(handle: &WolfCoreHandle, s: &str) {
    for c in s.chars() {
        handle.send_key(key(KIND_CHAR, c as u32, false));
    }
}

fn send_ex_command(handle: &WolfCoreHandle, cmd: &str) {
    send_chars(handle, cmd);
    handle.send_key(key(KIND_ENTER, 0, false));
}

fn send_esc(handle: &WolfCoreHandle) {
    handle.send_key(key(KIND_ESC, 0, false));
}

fn send_ctrl(handle: &WolfCoreHandle, c: char) {
    handle.send_key(key(KIND_CHAR, c as u32, true));
}

/// The stress document. Mirrors the shape of the report's document: prose
/// paragraphs interleaved with fenced starlark blocks (braces, quotes, commas).
/// Every line is well under 80 columns so nothing soft-wraps (display row ==
/// buffer line, which the viewport mapping below relies on), and blocks contain
/// no blank lines. 0-based lines 24..=31 and 42..=45 are the two blocks'
/// fence-to-fence ranges (checked by an assertion in the test against
/// `block_ranges`, so a doc edit that breaks the map fails loudly).
const DOC: &str = "\
# Bundled fonts

This paragraph explains the bundling. The inner fonts directory is
deliberate: the ruleset strips the bundled artifact's own top-level
directory name when copying it into the app resources.

By nesting everything under fonts, the app reliably gets the fonts
tree where the font registration looks and the bundle-layout test
expects them, robustly even if a future release adds a new entry.

Some more prose to push the blocks apart, so that scrolling between
them takes several steps and exercises grid_scroll in both
directions with the code blocks partially visible.

- a list item, for variety
- another list item

A short paragraph.

More prose above the first block. The block that follows mimics the
starlark snippet from the bug report, braces and quotes included.

## First block

```starlark
replace_prefixes = {
    \"fonts/ttf\": \"fonts\",
    \"OFL.txt\": \"fonts/OFL.txt\",
}
other = use_extension(\"//bazel/fonts:extensions.bzl\", \"fonts\")
use_repo(other, \"fonts\")
```

The build-time-only ruleset never enters the dependency graph. The
module file wires the extension up, as the next block shows.

Between the blocks there is enough prose that the two blocks do not
share a screen at 24 rows, which forces scrolling to move between
them and repaints the hl_eol fill as rows enter the viewport.

## Second block

```starlark
jetbrains = use_extension(\"//bazel/jb:extensions.bzl\", \"jb\")
use_repo(jetbrains, \"jetbrains_mono\")
```

Trailing prose so the document is comfortably longer than the
window, giving G / gg / PageUp / PageDown room to work.

The very last line of the document.
";

/// 0-based inclusive line ranges of the fenced blocks in [`DOC`], fences
/// included — computed the same way orlando's `find_blocks` does.
fn block_ranges(lines: &[&str]) -> Vec<RangeInclusive<usize>> {
    let mut blocks = Vec::new();
    let mut open: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if t.starts_with("```") {
            match open.take() {
                None => open = Some(i),
                Some(from) => blocks.push(from..=i),
            }
        }
    }
    blocks
}

/// One resolved cell as the invariant checker sees it.
#[derive(Clone, PartialEq, Eq, Hash)]
struct CheckCell {
    text: String,
    bg: Option<u32>,
}

/// A stable digest of the window frame (cells + viewport + size), used to
/// detect idleness (two ops' worth of redraw churn has fully settled).
fn frame_digest(handle: &WolfCoreHandle, grid: u64) -> u64 {
    let f = handle.read_frame_for_grid(grid);
    let mut h = DefaultHasher::new();
    (f.cols(), f.rows(), f.topline(), f.botline()).hash(&mut h);
    for cell in f.cells() {
        cell.text.hash(&mut h);
        let bg = cell.bg();
        (bg.set, bg.rgb).hash(&mut h);
        let fg = cell.fg();
        (fg.set, fg.rgb).hash(&mut h);
        cell.flags().hash(&mut h);
    }
    h.finish()
}

/// Waits until the frame digest is stable for [`IDLE_STREAK`] consecutive polls
/// (or [`IDLE_TIMEOUT`] passes — the caller's check then runs on whatever state
/// is there, which can only produce a MORE conservative failure report).
fn settle(handle: &WolfCoreHandle, grid: u64) {
    let deadline = Instant::now() + IDLE_TIMEOUT;
    let mut last = frame_digest(handle, grid);
    let mut streak = 1;
    while Instant::now() < deadline {
        sleep(POLL_INTERVAL);
        let d = frame_digest(handle, grid);
        if d == last {
            streak += 1;
            if streak >= IDLE_STREAK {
                return;
            }
        } else {
            streak = 1;
            last = d;
        }
    }
}

/// Returns the first visible (non-hidden) WINDOW grid id.
fn window_grid(handle: &WolfCoreHandle) -> u64 {
    let wins = handle.windows();
    let grid = wins.iter().find(|w| !w.hidden()).map_or(0, |w| w.grid());
    assert!(grid >= 2, "expected a window grid (>= 2), got {grid}");
    grid
}

/// Checks the two invariants over the currently visible window grid. Returns a
/// list of human-readable violations (empty = pass).
fn check_invariants(
    handle: &WolfCoreHandle,
    grid: u64,
    doc_lines: &[&str],
    blocks: &[RangeInclusive<usize>],
) -> Vec<String> {
    let f = handle.read_frame_for_grid(grid);
    let cols = f.cols() as usize;
    let rows = f.rows() as usize;
    let cells = f.cells();
    let topline = f.topline() as usize;
    let default_bg = {
        let c = f.default_bg();
        c.set.then_some(c.rgb)
    };
    let mut violations = Vec::new();

    // Resolve each row into text + per-cell bg.
    let row_cells: Vec<Vec<CheckCell>> = (0..rows)
        .map(|r| {
            (0..cols)
                .map(|c| {
                    let cell = &cells[r * cols + c];
                    let bg = cell.bg();
                    CheckCell {
                        text: cell.text.clone(),
                        bg: bg.set.then_some(bg.rgb),
                    }
                })
                .collect()
        })
        .collect();

    let row_text = |r: usize| -> String {
        let mut s = String::new();
        for cell in &row_cells[r] {
            if cell.text.is_empty() {
                s.push(' ');
            } else {
                s.push_str(&cell.text);
            }
        }
        s.trim_end().to_string()
    };

    // The block background: taken from the first visible block row's column 0.
    // All block cells must match it, and it must differ from the default bg
    // (otherwise orlando isn't painting and the whole test would be vacuous).
    let mut block_bg: Option<Option<u32>> = None;

    for r in 0..rows {
        let bufline = topline + r;
        if bufline >= doc_lines.len() {
            break; // end-of-buffer filler rows
        }

        // Invariant 1: text integrity.
        let got = row_text(r);
        let want = doc_lines[bufline].trim_end();
        if got != want {
            violations.push(format!(
                "TEXT row {r} (buffer line {bufline}): got {got:?}, want {want:?}"
            ));
            continue; // bg comparison is meaningless if the text already differs
        }

        let in_block = blocks.iter().any(|b| b.contains(&bufline));
        if !in_block {
            continue;
        }

        let expected = *block_bg.get_or_insert_with(|| row_cells[r][0].bg);
        for (c, cell) in row_cells[r].iter().enumerate() {
            if cell.bg != expected {
                violations.push(format!(
                    "BG row {r} (buffer line {bufline}) col {c} text {:?}: \
                     bg {:?} != block bg {expected:?}",
                    cell.text, cell.bg
                ));
            }
        }
    }

    if let Some(bg) = block_bg {
        if bg == default_bg {
            violations.push(format!(
                "VACUOUS: block bg {bg:?} equals default bg {default_bg:?} — \
                 orlando's code-block highlight is not being painted at all"
            ));
        }
    }

    violations
}

/// Tiny deterministic LCG so failures are reproducible from the printed seed.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn pick(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[test]
fn orlando_code_block_bg_stays_uniform_under_churn() {
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };
    std::env::set_var("WOLF_NVIM_BIN", &nvim_bin);

    let Some(orlando_tree) = common::orlando_dir_or_none() else {
        eprintln!(
            "\nSKIP: wolf-ffi orlando_bg_stress — WOLF_TEST_ORLANDO_DIR not staged \
             (vendored orlando plugin is Bazel/macOS-only). No-op pass.\n"
        );
        return;
    };

    // --- Stage the bundle layout (init.lua + sibling orlando/ [+ lighthouse/]) --
    let orlando_src = {
        let inner = orlando_tree.join("orlando");
        if inner.is_dir() {
            inner
        } else {
            orlando_tree.clone()
        }
    };
    let tmp = tempfile::tempdir().expect("failed to create temp staging dir");
    let root = tmp.path();
    std::fs::copy(common::init_lua_path(), root.join("init.lua"))
        .expect("failed to copy staged init.lua");
    common::copy_dir_recursive(&orlando_src, &root.join("orlando"))
        .expect("failed to copy the orlando tree");
    if let Some(lh) = common::lighthouse_dir_or_none() {
        let inner = lh.join("lighthouse");
        let src = if inner.is_dir() { inner } else { lh };
        common::copy_dir_recursive(&src, &root.join("lighthouse"))
            .expect("failed to copy the lighthouse tree");
    }

    let doc_path = root.join("doc.md");
    std::fs::write(&doc_path, DOC).expect("failed to write the stress document");

    let doc_lines: Vec<&str> = DOC.lines().collect();
    let blocks = block_ranges(&doc_lines);
    assert_eq!(blocks.len(), 2, "the stress doc should contain two blocks");
    // 0-based fence rows; `A`-append / `dd`+`u` targets below depend on these.
    assert_eq!(blocks[0], 24..=31, "first block moved — update the ops");
    assert_eq!(blocks[1], 42..=45, "second block moved — update the ops");

    let handle = wolf_ffi_start(
        root.join("init.lua").to_string_lossy().into_owned(),
        root.to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle");

    // --- Attach, open the doc, wait for orlando to paint --------------------
    let deadline = Instant::now() + ATTACH_TIMEOUT;
    while Instant::now() < deadline && handle.read_frame().cols() == 0 {
        sleep(POLL_INTERVAL);
    }
    assert!(handle.read_frame().cols() > 0, "core should attach");

    send_ex_command(&handle, ":e doc.md");
    let grid = {
        // The window grid can appear a beat after the :e round-trips.
        let deadline = Instant::now() + ATTACH_TIMEOUT;
        loop {
            let wins = handle.windows();
            if wins.iter().any(|w| !w.hidden()) {
                break window_grid(&handle);
            }
            assert!(Instant::now() < deadline, "no window grid appeared");
            sleep(POLL_INTERVAL);
        }
    };
    settle(&handle, grid);

    // The doc must actually be showing (guards against :e failing silently or
    // still being in flight — keystrokes round-trip asynchronously, so poll).
    let deadline = Instant::now() + ATTACH_TIMEOUT;
    let mut first_check = check_invariants(&handle, grid, &doc_lines, &blocks);
    while !first_check.is_empty() && Instant::now() < deadline {
        sleep(POLL_INTERVAL);
        first_check = check_invariants(&handle, grid, &doc_lines, &blocks);
    }
    assert!(
        first_check.is_empty(),
        "baseline state after opening the doc is already wrong \
         (test setup problem, not the bug):\n{}",
        first_check.join("\n")
    );

    // --- The stress loop -----------------------------------------------------
    let seed = std::env::var("WOLF_STRESS_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0x5EED_0001_u64);
    let iters: usize = std::env::var("WOLF_STRESS_ITERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    eprintln!("orlando_bg_stress: seed={seed} iters={iters}");
    let mut rng = Lcg(seed);
    let mut history: Vec<String> = Vec::new();

    for iter in 0..iters {
        let op = rng.pick(8);
        let desc = match op {
            0 => {
                let n = 1 + rng.pick(8);
                for _ in 0..n {
                    send_ctrl(&handle, 'e');
                }
                format!("<C-e> x{n}")
            }
            1 => {
                let n = 1 + rng.pick(8);
                for _ in 0..n {
                    send_ctrl(&handle, 'y');
                }
                format!("<C-y> x{n}")
            }
            2 => {
                send_chars(&handle, "G");
                "G".to_string()
            }
            3 => {
                send_chars(&handle, "gg");
                "gg".to_string()
            }
            4 => {
                let n = 1 + rng.pick(15);
                let c = if rng.pick(2) == 0 { "j" } else { "k" };
                for _ in 0..n {
                    send_chars(&handle, c);
                }
                format!("{c} x{n}")
            }
            5 => {
                // Append + undo on a line INSIDE the first block (1-based 26 =
                // 0-based 25, a code line). Net-neutral.
                send_chars(&handle, "26G");
                send_chars(&handle, "A x");
                send_esc(&handle);
                if rng.pick(2) == 0 {
                    settle(&handle, grid);
                }
                send_chars(&handle, "u");
                "26G A x <Esc> u".to_string()
            }
            6 => {
                // Insert an unterminated fence on a prose line (1-based 18, a
                // short paragraph) — everything below turns gray — then delete
                // it. Sometimes without settling in between (the racy path).
                send_chars(&handle, "18G");
                send_chars(&handle, "o```");
                send_esc(&handle);
                if rng.pick(2) == 0 {
                    settle(&handle, grid);
                }
                send_chars(&handle, "dd");
                "18G o``` <Esc> [settle?] dd".to_string()
            }
            _ => {
                // dd + u on a code line inside the second block (1-based 44).
                send_chars(&handle, "44G");
                send_chars(&handle, "dd");
                if rng.pick(2) == 0 {
                    settle(&handle, grid);
                }
                send_chars(&handle, "u");
                "44G dd [settle?] u".to_string()
            }
        };
        history.push(format!("iter {iter}: {desc}"));

        // Let the churn finish, then park the cursor on a blank line (the `{`
        // paragraph motion) so neither matchparen nor the cursor cell can
        // legitimately recolor a block cell, and check at true idle.
        settle(&handle, grid);
        send_esc(&handle);
        send_chars(&handle, "{");
        settle(&handle, grid);

        let violations = check_invariants(&handle, grid, &doc_lines, &blocks);
        if !violations.is_empty() {
            let f = handle.read_frame_for_grid(grid);
            eprintln!(
                "\n==== REPRODUCED at iter {iter} (seed {seed}) ====\n\
                 viewport topline={} botline={} rows={} cols={}\n\
                 op history:\n  {}\n\nviolations:\n  {}\n",
                f.topline(),
                f.botline(),
                f.rows(),
                f.cols(),
                history.join("\n  "),
                violations.join("\n  ")
            );
            panic!(
                "code-block background invariant violated at IDLE — wolf-core's \
                 grid state diverged from Neovim's (see dump above)"
            );
        }
    }

    handle.shutdown();
    drop(tmp);
}
