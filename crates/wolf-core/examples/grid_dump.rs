//! Headless smoke example: start the core against the pinned nvim, type a little
//! text, wait for a flush, and dump the grid + cursor + a sampled cell's resolved
//! colors. Run with:
//!
//! ```text
//! cargo run -p wolf-core --example grid_dump
//! ```
//!
//! Requires `WOLF_NVIM_BIN` (the flake devShell exports it) and the in-repo
//! `runtime/nvim/init.lua`.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use wolf_core::{CoreConfig, InputEvent, Key, KeyEvent, WakeFn, WolfCore};

const COLS: u16 = 80;
const ROWS: u16 = 24;
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

fn init_lua_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .join("..")
        .join("..")
        .join("runtime")
        .join("nvim")
        .join("init.lua")
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
    if std::env::var("WOLF_NVIM_BIN").map(|v| v.trim().is_empty()).unwrap_or(true) {
        eprintln!(
            "WOLF_NVIM_BIN is not set. Enter the flake devShell (`direnv allow`) first."
        );
        std::process::exit(1);
    }

    let (tx, rx): (Sender<()>, Receiver<()>) = mpsc::channel();
    let wake: WakeFn = Arc::new(move || {
        let _ = tx.send(());
    });

    let cfg = CoreConfig {
        init_lua: init_lua_path(),
        cols: COLS,
        rows: ROWS,
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
    rx.recv_timeout(FLUSH_TIMEOUT).expect("no flush after typing");
    while rx.recv_timeout(Duration::from_millis(200)).is_ok() {}

    {
        let snap = core.read_snapshot();
        println!("grid {}x{}, seq={}", snap.grid.cols, snap.grid.rows, snap.seq);
        println!(
            "cursor row={} col={} mode_idx={}",
            snap.cursor.row, snap.cursor.col, snap.cursor.mode_idx
        );
        println!("--- grid rows ---");
        for row in 0..snap.grid.rows {
            let text = snap.grid.row_text(row);
            if text.trim().is_empty() {
                continue;
            }
            println!("{row:>3}| {text}");
        }

        // Sample the first non-space cell on row 0 and show its resolved colors.
        let row0 = snap.grid.row_text(0);
        if let Some(col) = row0.char_indices().find(|(_, c)| !c.is_whitespace()).map(|(i, _)| i) {
            if let Some(cell) = snap.grid.cell(0, col) {
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
