//! Shared helpers for `wolf-core` integration tests.
//!
//! These tests drive a REAL, hermetic Neovim process (no mocks). The nvim binary
//! and the captive `init.lua` are resolved in one of two ways:
//!
//! 1. **Cargo / Linux devShell:** the flake devShell exports an explicit,
//!    ALREADY-ABSOLUTE `WOLF_NVIM_BIN`, and `init.lua` is found relative to
//!    `CARGO_MANIFEST_DIR`. This path is unchanged and always wins when present.
//! 2. **Bazel (`bazel test //crates/wolf-core:file_search`):** the `rust_test`
//!    stages the pinned `//bazel/nvim:nvim_tree` and `//runtime/nvim:init.lua` as
//!    runfiles and injects RUNFILES-RELATIVE paths via `WOLF_TEST_NVIM_DIR` /
//!    `WOLF_TEST_INIT_LUA` (and a relative `WOLF_NVIM_BIN`). These are resolved to
//!    ABSOLUTE paths against `TEST_SRCDIR` / `RUNFILES_DIR` (see [`resolve_runfile`]).
//!
//! This mirrors `crates/wolf-ffi/tests/common/mod.rs` (`resolve_runfile`,
//! `nvim_bin_or_skip`, `init_lua_path`). The one deliberate difference: wolf-ffi
//! returns `None` so its caller can SKIP, whereas the wolf-core test bodies do not
//! handle a skip return, so [`require_nvim_bin`] still PANICS as the last resort
//! when nothing resolves. On both real environments (cargo devShell, Bazel host)
//! resolution always succeeds, so the panic never fires there.
//!
//! Public API surface consumed here mirrors the plan's `src/api` + `src/input`
//! re-exports. Where a method name is genuinely ambiguous in the plan a
//! `// NOTE:` comment flags the assumption for the coder to align.

#![allow(dead_code)] // Not every helper is used by every test binary.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{mpsc, Arc, OnceLock};
use std::time::{Duration, Instant};

use std::io::Write as _;

use wolf_core::{CoreConfig, InputEvent, Key, KeyEvent, WakeFn, WolfCore};

/// Default grid width used by most tests, in columns.
pub const DEFAULT_COLS: u16 = 80;
/// Default grid height used by most tests, in rows.
pub const DEFAULT_ROWS: u16 = 24;

/// Generous timeout to absorb nvim `--embed` startup + first redraw flush.
/// nvim startup is not instantaneous and redraw is never synchronous, so we wait
/// well beyond the expected latency before declaring failure.
pub const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Resolves a data file/dir the test needs at RUNTIME from a Bazel-injected env
/// var holding a runfiles-relative path (set via `$(rootpath …)` in the
/// `rust_test`'s `env`). Under Bazel the file lives in the test's runfiles tree,
/// whose root differs by Bazel version and (for external repos) carries a repo
/// segment — so we probe the known bases (`TEST_SRCDIR`, `+ TEST_WORKSPACE`,
/// `+ _main`, `RUNFILES_DIR`, the cwd) and return the first that exists.
///
/// Returns `None` when the env var is unset (the cargo / Linux-devShell path,
/// where callers fall back to `CARGO_MANIFEST_DIR` or an explicit `WOLF_NVIM_BIN`)
/// or when nothing resolves.
///
/// This is a byte-for-byte port of `crates/wolf-ffi/tests/common/mod.rs`'s
/// `resolve_runfile`, so the two harnesses resolve staged runfiles identically.
fn resolve_runfile(env_key: &str) -> Option<PathBuf> {
    let rel = std::env::var(env_key).ok().filter(|v| !v.is_empty())?;
    let relp = PathBuf::from(&rel);
    if relp.is_absolute() {
        return relp.exists().then_some(relp);
    }

    let mut bases: Vec<PathBuf> = Vec::new();
    if let Ok(srcdir) = std::env::var("TEST_SRCDIR") {
        if let Ok(wsp) = std::env::var("TEST_WORKSPACE") {
            bases.push(PathBuf::from(&srcdir).join(&wsp));
        }
        bases.push(PathBuf::from(&srcdir).join("_main"));
        bases.push(PathBuf::from(&srcdir));
    }
    if let Ok(rfdir) = std::env::var("RUNFILES_DIR") {
        bases.push(PathBuf::from(&rfdir).join("_main"));
        bases.push(PathBuf::from(&rfdir));
    }
    if let Ok(cwd) = std::env::current_dir() {
        bases.push(cwd);
    }

    bases
        .into_iter()
        .map(|b| b.join(&relp))
        .find(|c| c.exists())
}

/// Resolves a usable pinned-nvim binary as an ABSOLUTE path, mirroring
/// `crates/wolf-ffi/tests/common/mod.rs`'s `nvim_bin_or_skip` resolution order:
///
/// 1. An explicit `WOLF_NVIM_BIN` that is non-empty, ABSOLUTE, and EXISTS — the
///    cargo / Linux-devShell path. It wins so a developer can point at a specific
///    nvim. (Unlike wolf-ffi we require absolute-and-existing here: the wolf-core
///    Bazel `rust_test` injects a RUNFILES-RELATIVE `WOLF_NVIM_BIN`, which would
///    fail to spawn once nvim is launched with `current_dir(cfg.cwd)`; so a
///    relative/non-existent value is NOT trusted and we fall through to the staged
///    tree below.)
/// 2. The Bazel-staged pinned Neovim tree (`WOLF_TEST_NVIM_DIR` → the
///    `//bazel/nvim:nvim_tree` runfiles dir), whose binary is `nvim/bin/nvim`
///    (the `replace_prefixes` inner `nvim/`), with `bin/nvim` as a fallback.
///
/// Returns `None` when neither is available. The caller ([`require_nvim_bin`])
/// turns that into a loud panic, since the wolf-core test bodies do not handle a
/// skip return; on both real environments resolution always succeeds.
fn resolve_nvim_bin() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("WOLF_NVIM_BIN") {
        let p = PathBuf::from(v.trim());
        if p.is_absolute() && p.exists() {
            return Some(p);
        }
    }

    if let Some(dir) = resolve_runfile("WOLF_TEST_NVIM_DIR") {
        // The tree is `nvim_tree/nvim/bin/nvim` (the `replace_prefixes` inner
        // `nvim/`); accept a flat `bin/nvim` too, defensively.
        let candidates = [
            dir.join("nvim").join("bin").join("nvim"),
            dir.join("bin").join("nvim"),
        ];
        if let Some(bin) = candidates.into_iter().find(|b| b.exists()) {
            return Some(bin);
        }
    }

    None
}

/// Resolves the pinned nvim binary to an ABSOLUTE path and EXPORTS it back into
/// `WOLF_NVIM_BIN`, then returns it. Panics with a clear, actionable message when
/// no nvim can be resolved.
///
/// wolf-core locates nvim ONLY through the `WOLF_NVIM_BIN` env var (read by
/// `NvimConfig::from_env`), and the captive nvim is now spawned with
/// `current_dir(cfg.cwd)` (a temp dir). A relative `WOLF_NVIM_BIN` (as injected by
/// the Bazel `rust_test`) would therefore resolve against that temp dir and fail
/// to spawn. So — exactly like `wolf-ffi`'s `bridge_smoke` does with the result of
/// `nvim_bin_or_skip` — we resolve an absolute path here and `set_var` it so the
/// core spawns the right binary regardless of cwd. This is idempotent: once set,
/// subsequent calls take the absolute-and-existing fast path in [`resolve_nvim_bin`].
///
/// Returns the resolved binary path for convenience.
pub fn require_nvim_bin() -> PathBuf {
    match resolve_nvim_bin() {
        Some(bin) => {
            std::env::set_var("WOLF_NVIM_BIN", &bin);
            bin
        }
        None => panic!(
            "No pinned Neovim could be resolved. These integration tests require a \
             real, pinned Neovim binary. Provide one of:\n\
             - an ABSOLUTE, existing WOLF_NVIM_BIN (exported by the flake devShell; \
             enter it via `direnv allow`), or\n\
             - the Bazel-staged tree via WOLF_TEST_NVIM_DIR (run via \
             `bazel test //crates/wolf-core:file_search`).\n\
             nvim is NOT taken from PATH."
        ),
    }
}

/// Resolves the captive `runtime/nvim/init.lua`.
///
/// Prefers the Bazel-staged runfile (`WOLF_TEST_INIT_LUA` → `$(rootpath
/// //runtime/nvim:init.lua)`, resolved to an ABSOLUTE path); under cargo it falls
/// back to `CARGO_MANIFEST_DIR` (which points at `crates/wolf-core`), i.e.
/// `<manifest>/../../runtime/nvim/init.lua`.
///
/// This matters: under Bazel, `CARGO_MANIFEST_DIR` is an EPHEMERAL build-sandbox
/// path that does not exist at test runtime, so launching nvim with `-u <that>`
/// makes nvim fail to read its init and block on a "Press ENTER" prompt forever
/// (hanging `WolfCore::start`). Resolving the staged runfile fixes that. Mirrors
/// `crates/wolf-ffi/tests/common/mod.rs`'s `init_lua_path`.
pub fn init_lua_path() -> PathBuf {
    if let Some(p) = resolve_runfile("WOLF_TEST_INIT_LUA") {
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

/// Monotonic counter to disambiguate temp `cwd`s created within one process.
static CWD_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Creates a fresh, unique directory under the OS temp dir to serve as a core
/// `cwd`, and returns its path.
///
/// The directory is intentionally NOT auto-deleted: [`config`] returns only a
/// `CoreConfig`, so there is no `TempDir` guard to keep alive. The recent-files
/// MRU now lives in a single global DB at `state_dir()/file-mru.sqlite`
/// (directory-filtered), NOT under the cwd; this dir is still leaked so it does
/// not vanish out from under `WolfCore::start` (nvim's working dir + search root).
/// Leaking one small temp dir per test is acceptable for this hermetic suite.
pub fn fresh_cwd() -> PathBuf {
    let n = CWD_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time is before the unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("wolf-test-cwd-{pid}-{nanos}-{n}"));
    std::fs::create_dir_all(&dir).expect("failed to create temp cwd directory");
    dir
}

/// Builds a `CoreConfig` with the resolved init.lua path, the given grid size,
/// and a fresh per-call `cwd` (so each core gets its own working directory).
///
/// NOTE: assumes `CoreConfig` has public fields `init_lua`, `cols`, `rows`, and
/// (new in S1) `cwd` (per plan §D6). If `CoreConfig` instead exposes a
/// builder/constructor, the coder should adapt [`config_in`] (the single literal).
pub fn config(cols: u16, rows: u16) -> CoreConfig {
    config_in(fresh_cwd(), cols, rows)
}

/// Process-global hermetic state dir, created ONCE per test process.
///
/// The global recent-directories store is always-on: it reads `state_dir()` and
/// writes `folder-mru.sqlite` there. Without an override that would be the user's
/// real `~/Library/Application Support/Wolf`, polluting it (and racing) across the
/// whole real-nvim suite. We therefore point `WOLF_STATE_DIR` at a fresh temp dir
/// for the process (see [`ensure_hermetic_state_dir`]).
///
/// The `TempDir` guard is stored in the `OnceLock` so it lives for the entire
/// process: statics are NOT dropped at process exit, so the directory is never
/// auto-deleted out from under a running core.
static HERMETIC_STATE_DIR: OnceLock<tempfile::TempDir> = OnceLock::new();

/// Exports a process-global temp `WOLF_STATE_DIR` **only if it is unset**, so the
/// always-on recent-directories store writes into a throwaway dir instead of the
/// user's real state dir.
///
/// Set-if-absent: a test that sets its own `WOLF_STATE_DIR` first (e.g. the
/// recent-dirs real-nvim test, which needs to inspect the physical DB) is never
/// clobbered. The value is stable for the process, so there is no cross-test race.
fn ensure_hermetic_state_dir() {
    if std::env::var_os("WOLF_STATE_DIR").is_some() {
        return;
    }
    let dir = HERMETIC_STATE_DIR
        .get_or_init(|| tempfile::tempdir().expect("failed to create hermetic state dir"));
    // Re-check under the (effectively) initialized value: another thread may have
    // set WOLF_STATE_DIR between the check above and here; set-if-absent is fine
    // because the process-global value is stable regardless of which we pick.
    if std::env::var_os("WOLF_STATE_DIR").is_none() {
        std::env::set_var("WOLF_STATE_DIR", dir.path());
    }
}

/// Resolves the process-global hermetic state dir as a `PathBuf`, ensuring
/// `WOLF_STATE_DIR` first points at a throwaway dir (see
/// [`ensure_hermetic_state_dir`]) and then returning `wolf_core::state_dir()`.
///
/// The recent-files MRU now lives in a SINGLE global DB at
/// `state_dir()/file-mru.sqlite` (directory-filtered), so real-nvim tests that
/// must pre-seed or inspect the MRU open a `MruStore` against THIS dir — the same
/// dir the core resolves via `state_dir()` — rather than a per-cwd `.wolf`.
pub fn hermetic_state_dir() -> PathBuf {
    ensure_hermetic_state_dir();
    wolf_core::state_dir().expect("state_dir() should resolve once WOLF_STATE_DIR is set")
}

/// Like [`config`] but with an explicit `cwd`, for tests that must control nvim's
/// working directory — the search root and the `directory` scope under which
/// opens are recorded into the single global MRU at `state_dir()/file-mru.sqlite`
/// — so they can drive it and inspect the results.
pub fn config_in(cwd: PathBuf, cols: u16, rows: u16) -> CoreConfig {
    require_nvim_bin();
    // Keep the always-on recent-directories store off the user's real state dir
    // (set-if-absent, so a test's own WOLF_STATE_DIR wins).
    ensure_hermetic_state_dir();
    CoreConfig {
        init_lua: init_lua_path(),
        cols,
        rows,
        cwd,
    }
}

/// A wait handle backed by an mpsc channel. The wake callback sends `()` on
/// EVERY wake — both real redraw flushes and out-of-band wakes such as the
/// `wolf_current_file` notification. To distinguish a genuine redraw flush from
/// an out-of-band wake, `wait_for_flush` keys on the snapshot's `seq` (the redraw
/// frame counter, bumped only on a real flush) ADVANCING, not on the raw wake
/// count. A `current_file` change wakes but does NOT bump `seq`, so it is ignored
/// here while still letting `current_file.rs` observe it by polling directly.
pub struct FlushWaiter {
    rx: Receiver<()>,
}

impl FlushWaiter {
    /// Blocks until a genuine redraw flush lands, i.e. until `core`'s snapshot
    /// `seq` advances beyond its value at the moment of this call. Out-of-band
    /// wakes (e.g. `wolf_current_file`, which does not bump `seq`) are consumed
    /// and ignored so they cannot be mistaken for a flush. Panics after `timeout`.
    pub fn wait_for_flush(&self, core: &WolfCore, timeout: Duration) {
        let baseline = core.read_snapshot().seq;
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(remaining) {
                Ok(()) => {
                    // A wake arrived; it is only a real flush if seq advanced.
                    if core.read_snapshot().seq > baseline {
                        return;
                    }
                    // Out-of-band wake (e.g. current_file): keep waiting.
                }
                Err(RecvTimeoutError::Timeout) => panic!(
                    "timed out after {timeout:?} waiting for a redraw flush from nvim \
                     (snapshot seq never advanced past {baseline})"
                ),
                Err(RecvTimeoutError::Disconnected) => {
                    panic!("wake channel disconnected before a flush arrived (nvim/core gone?)")
                }
            }
        }
    }

    /// Blocks until the FIRST redraw flush has landed, i.e. until `core`'s
    /// snapshot `seq` reaches at least 1.
    ///
    /// Unlike [`wait_for_flush`](Self::wait_for_flush), this keys on an ABSOLUTE
    /// threshold (`seq >= 1`) rather than on `seq` advancing from a captured
    /// baseline. That is required for the startup/attach wait: the attach redraw
    /// flush (`seq` 0 -> 1) lands DURING `WolfCore::start` (it `block_on`s
    /// `ui_attach` before returning), so by the time a test calls this the snapshot
    /// is already at `seq == 1`. An advance-based wait would baseline at 1 and then
    /// block forever for `seq > 1`. This method therefore RETURNS IMMEDIATELY when
    /// `seq >= 1` already (the common case), and only otherwise blocks on the wake
    /// channel. Out-of-band wakes (e.g. `wolf_current_file`, which does not bump
    /// `seq`) are consumed and ignored. Panics after `timeout`.
    pub fn wait_for_initial_flush(&self, core: &WolfCore, timeout: Duration) {
        const INITIAL_FLUSH_SEQ: u64 = 1;
        // Common case: the attach flush already landed during WolfCore::start.
        if core.read_snapshot().seq >= INITIAL_FLUSH_SEQ {
            return;
        }
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(remaining) {
                Ok(()) => {
                    // A wake arrived; the attach flush only counts once seq >= 1.
                    if core.read_snapshot().seq >= INITIAL_FLUSH_SEQ {
                        return;
                    }
                    // Out-of-band wake (e.g. current_file): keep waiting.
                }
                Err(RecvTimeoutError::Timeout) => panic!(
                    "timed out after {timeout:?} waiting for the initial redraw flush \
                     from nvim (snapshot seq never reached {INITIAL_FLUSH_SEQ})"
                ),
                Err(RecvTimeoutError::Disconnected) => panic!(
                    "wake channel disconnected before the initial flush arrived \
                     (nvim/core gone?)"
                ),
            }
        }
    }

    /// Drains any already-queued wake signals without blocking. Useful to
    /// discard wakes that occurred before a test starts observing.
    pub fn drain(&self) {
        while self.rx.try_recv().is_ok() {}
    }

    /// Blocks until ANY raw wake signal arrives, returning `true` on success or
    /// `false` after `timeout`.
    ///
    /// Unlike [`wait_for_flush`](Self::wait_for_flush), this does NOT require the
    /// snapshot `seq` to advance: it observes the wake SEAM directly. It exists
    /// for the liveness tests, where a state transition (e.g. nvim death, marked
    /// `Dead` by the supervisor) fires a wake so the front-end can re-read
    /// `liveness()`, but does NOT bump `seq` (it is not a redraw frame). Callers
    /// that want to attribute the wake to a specific episode should [`drain`]
    /// first.
    pub fn wait_for_wake(&self, timeout: Duration) -> bool {
        matches!(self.rx.recv_timeout(timeout), Ok(()))
    }
}

/// Builds a `WakeFn` (`Arc<dyn Fn() + Send + Sync>`) that signals on each flush,
/// paired with the `FlushWaiter` that observes it.
///
/// The wake may fire on a tokio worker thread, so the closure must be
/// `Send + Sync`; an mpsc `Sender<()>` is both.
pub fn wake_channel() -> (WakeFn, FlushWaiter) {
    let (tx, rx): (Sender<()>, Receiver<()>) = mpsc::channel();
    let wake: WakeFn = Arc::new(move || {
        // A send error only means the receiver was dropped (test finished); ignore.
        let _ = tx.send(());
    });
    (wake, FlushWaiter { rx })
}

/// Writes `content` to a fresh temp file with a `.md` extension and returns it.
/// The returned `NamedTempFile` keeps the file alive for the test's duration.
pub fn temp_md(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new()
        .prefix("wolf-test-")
        .suffix(".md")
        .tempfile()
        .expect("failed to create temp .md file");
    f.write_all(content.as_bytes())
        .expect("failed to write temp .md content");
    f.flush().expect("failed to flush temp .md content");
    f
}

/// Allocates a path for a not-yet-existing `.md` file (for SaveAs). The file is
/// NOT created on disk; the directory guard keeps the parent alive.
pub fn temp_md_path() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let path = dir.path().join("wolf-saveas.md");
    (dir, path)
}

/// Starts a `WolfCore` at default grid size, opens a temp `.md` containing
/// `content`, and waits for the first flush so the grid reflects the open file.
///
/// Returns the live core, its flush waiter, and the temp file (kept alive).
pub fn start_core_with_temp_md(content: &str) -> (WolfCore, FlushWaiter, tempfile::NamedTempFile) {
    start_core_with_temp_md_sized(content, DEFAULT_COLS, DEFAULT_ROWS)
}

/// Like `start_core_with_temp_md` but with an explicit grid size.
pub fn start_core_with_temp_md_sized(
    content: &str,
    cols: u16,
    rows: u16,
) -> (WolfCore, FlushWaiter, tempfile::NamedTempFile) {
    let file = temp_md(content);
    let (wake, waiter) = wake_channel();

    let core = WolfCore::start(config(cols, rows), wake)
        .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");

    // Wait for the initial attach/flush before opening so we don't race startup.
    // The attach flush (seq 0 -> 1) lands during WolfCore::start, so this keys on
    // the absolute threshold (seq >= 1) and returns immediately in the common case;
    // an advance-based wait would baseline at 1 and block forever.
    waiter.wait_for_initial_flush(&core, FLUSH_TIMEOUT);

    // Open the file; nvim will emit a redraw + flush once the buffer is shown.
    core.send_input(InputEvent::Open {
        path: file.path().to_path_buf(),
    });
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    (core, waiter, file)
}

/// Starts a bare `WolfCore` (no file opened) at the given grid size and waits
/// for the first flush.
pub fn start_core_sized(cols: u16, rows: u16) -> (WolfCore, FlushWaiter) {
    let (wake, waiter) = wake_channel();
    let core = WolfCore::start(config(cols, rows), wake)
        .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    // Attach flush (seq 0 -> 1) lands during WolfCore::start; key on seq >= 1.
    waiter.wait_for_initial_flush(&core, FLUSH_TIMEOUT);
    (core, waiter)
}

// --- Small input constructors to cut repetition ---------------------------

/// A plain (unmodified) key press.
pub fn key(k: Key) -> InputEvent {
    InputEvent::Key(KeyEvent {
        key: k,
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    })
}

/// A single plain character key press.
pub fn ch(c: char) -> InputEvent {
    key(Key::Char(c))
}

/// A Ctrl-modified key press.
pub fn ctrl(k: Key) -> InputEvent {
    InputEvent::Key(KeyEvent {
        key: k,
        ctrl: true,
        shift: false,
        alt: false,
        meta: false,
    })
}

/// Sends each character in `s` as an individual key event, in order.
pub fn type_str(core: &WolfCore, s: &str) {
    for c in s.chars() {
        core.send_input(ch(c));
    }
}
