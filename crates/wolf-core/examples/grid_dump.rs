//! Headless smoke example: start the core against the pinned nvim, type a little
//! text, wait for a flush, and dump the grid + cursor + a sampled cell's resolved
//! colors. Run with:
//!
//! ```text
//! cargo run -p wolf-core --example grid_dump      # Linux, in the flake devShell
//! bazel run //crates/wolf-core:grid_dump          # macOS, with staged runfiles
//! ```
//!
//! Requires `WOLF_NVIM_BIN` (the flake devShell exports it) and the in-repo
//! `runtime/nvim/init.lua`. Under `bazel run` both come from the target's runfiles:
//! `WOLF_NVIM_BIN` and `WOLF_TEST_INIT_LUA` hold paths relative to the runfiles root,
//! which is the working directory, and are resolved to absolute paths before start.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use wolf_core::{CoreConfig, InputEvent, Key, KeyEvent, WakeFn, WolfCore};

const COLS: u16 = 80;
const ROWS: u16 = 24;
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Prefers `WOLF_TEST_INIT_LUA` (set by the Bazel target), because under Bazel
/// `CARGO_MANIFEST_DIR` names the compile action's sandbox, gone at run time, and nvim
/// started with `-u <missing>` hangs on a Press ENTER prompt.
fn init_lua_path() -> PathBuf {
    if let Some(p) = env_path("WOLF_TEST_INIT_LUA") {
        return p;
    }
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .join("..")
        .join("..")
        .join("runtime")
        .join("nvim")
        .join("init.lua")
}

/// Reads a non-empty path from `var`, made absolute against the current directory.
fn env_path(var: &str) -> Option<PathBuf> {
    let raw = std::env::var_os(var).filter(|v| !v.is_empty())?;
    let path = PathBuf::from(raw);
    Some(std::fs::canonicalize(&path).unwrap_or(path))
}

fn key(k: Key) -> InputEvent {
    InputEvent::Key(KeyEvent {
        key: k,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    })
}

fn main() {
    if std::env::var("WOLF_NVIM_BIN")
        .map(|v| v.trim().is_empty())
        .unwrap_or(true)
    {
        eprintln!("WOLF_NVIM_BIN is not set. Enter the flake devShell (`direnv allow`) first.");
        std::process::exit(1);
    }
    // Bazel's WOLF_NVIM_BIN is runfiles-relative: make it absolute, as require_nvim_bin does.
    if let Some(bin) = env_path("WOLF_NVIM_BIN") {
        std::env::set_var("WOLF_NVIM_BIN", bin);
    }

    let (tx, rx): (Sender<()>, Receiver<()>) = mpsc::channel();
    let wake: WakeFn = Arc::new(move || {
        let _ = tx.send(());
    });

    let cfg = CoreConfig {
        init_lua: init_lua_path(),
        cols: COLS,
        rows: ROWS,
        cwd: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        terminfo_dir: None,
    };

    let core = WolfCore::start(cfg, wake).expect("WolfCore::start failed");

    // Wait for the initial attach flush.
    rx.recv_timeout(FLUSH_TIMEOUT).expect("no initial flush");

    // Type a heading.
    core.send_input(key(Key::Char('i')));
    for c in "# Hello, Wolf".chars() {
        core.send_input(key(Key::Char(c)));
    }
    core.send_input(key(Key::Esc));

    // Wait for the first post-typing flush, then keep draining brief follow-up
    // flushes so we observe the settled frame (insert -> normal mode emits a few).
    rx.recv_timeout(FLUSH_TIMEOUT)
        .expect("no flush after typing");
    while rx.recv_timeout(Duration::from_millis(200)).is_ok() {}

    {
        let snap = core.read_snapshot();
        let grid = snap.active_grid();
        println!(
            "active grid {} {}x{}, seq={}",
            snap.active_grid_id(),
            grid.cols,
            grid.rows,
            snap.seq
        );
        println!(
            "cursor grid={} row={} col={} mode_idx={}",
            snap.cursor.grid, snap.cursor.row, snap.cursor.col, snap.cursor.mode_idx
        );
        println!("--- grid rows ---");
        for row in 0..grid.rows {
            let text = grid.row_text(row);
            if text.trim().is_empty() {
                continue;
            }
            println!("{row:>3}| {text}");
        }

        // Sample the first non-space cell on row 0 and show its resolved colors.
        let row0 = grid.row_text(0);
        if let Some(col) = row0
            .char_indices()
            .find(|(_, c)| !c.is_whitespace())
            .map(|(i, _)| i)
        {
            if let Some(cell) = grid.cell(0, col) {
                let style = snap.highlights.resolve(cell.hl_id);
                println!(
                    "sampled cell (0,{col}) text={:?} hl_id={} fg={:?} bg={:?}",
                    cell.text, cell.hl_id, style.fg, style.bg
                );
            }
        }
        println!(
            "defaults fg={:?} bg={:?}",
            snap.highlights.defaults.fg, snap.highlights.defaults.bg
        );
    }

    core.shutdown().expect("shutdown should be Ok");
}
