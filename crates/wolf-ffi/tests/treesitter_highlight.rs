//! Integration test for **treesitter highlighting + fenced-code injection** in the
//! captive `runtime/nvim/init.lua`.
//!
//! This drives a REAL, hermetic Neovim — NO mocks, per the project testing policy
//! — and proves that the captive `init.lua`'s conditional treesitter block actually
//! makes markdown highlighting ACTIVE and that a fenced ```` ```go ```` block routes
//! through the embedded (injected) go parser. It uses the REAL captive `init.lua`
//! UNMODIFIED (resolved via `common::init_lua_path`), STAGED next to its vendored
//! siblings — the block self-locates those siblings relative to its own directory
//! (`debug.getinfo`), so a bare pointer at the staged trees would not exercise the
//! real code path. We therefore reconstruct the bundle layout in a temp dir T.
//!
//! ## What this proves (one precondition, one discriminator, one sentinel)
//!
//! After opening a markdown buffer the sentinel records:
//!   line 1: `vim.treesitter.highlighter.active[buf] ~= nil` — treesitter is attached
//!           to the buffer. This is a PRECONDITION, not evidence for the gate.
//!           MEASURED on the pinned nvim: a markdown buffer under `--clean -u NONE`,
//!           with no plugin and no gate at all, already reads OK, because nvim ships
//!           the markdown parser and starts treesitter on markdown BY ITSELF. An
//!           assertion resting on this line alone would pass under its own falsifier.
//!   line 2: after forcing a full parse (`p:parse(true)`), the top-level markdown
//!           langtree has an INJECTED `go` child — proving nvim-treesitter's
//!           `queries/markdown/injections.scm` AND the vendored `go` grammar both
//!           reached rtp, which is what routes the fenced block to the embedded
//!           parser. This is the DISCRIMINATING line: measured NO with the gate off
//!           and OK with it on. We prefer langtree presence over specific
//!           highlight-group names (which depend on the colorscheme's @capture links).
//!
//! ## Why the sibling staging (and the two vendored trees)
//!
//! The captive block GATES on a sibling `nvim-treesitter/` dir (the queries
//! provider) existing next to `init.lua`; when present it prepends `nvim-treesitter/`
//! (queries) and `parsers/` (compiled `<lang>.so`) to `runtimepath`. So we stage
//! BOTH: copy the staged `init.lua` → `T/init.lua`, recursively copy the staged
//! nvim-treesitter tree → `T/nvim-treesitter` and the staged parsers tree →
//! `T/parsers`, then launch nvim with `init_lua = T/init.lua`. Staging real siblings
//! is what makes this exercise the gate's FIRST resolution branch; the environment
//! fallback is the second test's subject. Where neither a sibling nor an environment
//! path resolves, the gate is a strict no-op: the buffer keeps the markdown
//! highlighting nvim starts on its own, and no fence is injected.
//!
//! ## SKIP-when-unset policy
//!
//! Like its siblings this test needs a pinned nvim (`nvim_bin_or_skip`); it ALSO
//! needs both trees (`nvim_treesitter_dir_or_none`, `parsers_dir_or_none`). Each
//! resolves from either provider — the Bazel-staged `WOLF_TEST_*` variables first,
//! then the Linux devShell's `WOLF_NVIM_TREESITTER_DIR` / `WOLF_PARSERS_DIR` — so it
//! ASSERTS on both platforms and is a loud no-op pass only on a host with neither.
//!
//! ## The second test: how the two trees are RESOLVED
//!
//! `vendored_dirs_resolve_sibling_first_and_a_stale_env_path_is_a_no_op` covers
//! the resolver in front of the gate rather than the gate's product, so it stages
//! its own empty decoy directories and needs NEITHER vendored tree. It skips only
//! when there is no pinned nvim.
//!
//! It also does not go through the FFI, and that is deliberate: it has to give the
//! child nvim a specific `WOLF_NVIM_TREESITTER_DIR` / `WOLF_PARSERS_DIR`, and the
//! bridge offers no per-spawn environment, so the only way through it would be
//! `std::env::set_var` on the whole test PROCESS. That would race the test above,
//! which reads both variables through `common::*_dir_or_none()` — and the losing
//! outcome is not a flake but a silent one: a decoy value that does not exist
//! makes those helpers return `None`, turning the test above into a loud-skip
//! PASS. So the child is spawned directly with a per-child environment, and this
//! test writes nothing to the process environment. The one process-wide write in
//! this binary is `nvim_bin_or_skip`'s publish of `WOLF_NVIM_BIN`, which is
//! conditional (nothing is written where the variable already holds the resolved
//! path, i.e. the whole devShell path) and happens once behind a `OnceLock` that
//! every reader is ordered after.
//!
//! Premises it relies on, each MEASURED by the probe rather than assumed:
//!
//! - `--clean` suppresses user config, plugins and shada but NOT the process
//!   environment, and `vim.env` is a table view over getenv. Sentinel lines 2 and
//!   3 read both variables back from inside the child, so a case cannot pass
//!   because the environment never arrived.
//! - Startup `-c` commands run after `init.lua` has been sourced and before
//!   `VimEnter`, so the gate's effect on `runtimepath` is already final when the
//!   probe reads it, and nvim quits before the untitled-buffer naming runs.
//! - The augroup is observed as "some `FileType`/`markdown` autocmd belongs to
//!   the `WolfTreesitter` group", NOT as `nvim_get_autocmds` raising for a
//!   missing group: the raise is version-dependent, and the two platforms are on
//!   different Neovim builds. The two cases assert opposite answers from the one
//!   probe, so a probe that answered the same way regardless would fail the first.

mod common;

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use wolf_ffi::{wolf_ffi_start, FfiKeyEvent, WolfCoreHandle};

/// Generous overall timeout for nvim `--embed` startup plus opening the markdown
/// buffer, parsing, and flushing the sentinel file. Mirrors the sibling tests'
/// `FLUSH_TIMEOUT`.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll cadence while waiting for attach / the sentinel file. Mirrors the sibling
/// tests' `POLL_INTERVAL`.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// -- FfiKeyEvent.kind discriminants (mirror `wolf-ffi/src/lib.rs` `mod kind`) --
/// A printable character (`ch` carries the Unicode scalar).
const KIND_CHAR: u8 = 0;
/// The Enter / Return key (executes the typed Ex command).
const KIND_ENTER: u8 = 1;

/// The two sentinel lines the `:lua` command writes: `[highlighter_active,
/// injected_go_langtree]`.
const EXPECTED_SENTINEL_LINES: usize = 2;
/// The marker the `:lua` command writes for a satisfied condition.
const MARK_OK: &str = "OK";

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

/// Types `cmd` as an Ex command and presses Enter: each char goes through the same
/// `send_key` / `FfiKeyEvent` path the generated Swift glue uses, then a final
/// `<CR>` executes it. `cmd` must include the leading `:`. Mirrors `panes.rs`.
fn send_ex_command(handle: &WolfCoreHandle, cmd: &str) {
    for c in cmd.chars() {
        handle.send_key(char_key(c));
    }
    handle.send_key(enter_key());
}

#[test]
fn markdown_treesitter_highlight_and_go_injection_active() {
    // Loud skip without a pinned nvim. No `set_var` here: the helper publishes `WOLF_NVIM_BIN`
    // race-free; see crates/wolf-ffi/tests/README.md#the-shared-harness--commonmodrs
    let Some(_nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };

    // Skip without an nvim-treesitter tree from either provider: the captive treesitter block is a
    // strict no-op without one.
    let Some(nt_dir) = common::nvim_treesitter_dir_or_none() else {
        eprintln!(
            "\nSKIP: wolf-ffi treesitter_highlight — no nvim-treesitter queries tree: neither \
             WOLF_TEST_NVIM_TREESITTER_DIR (Bazel) nor WOLF_NVIM_TREESITTER_DIR (Linux \
             devShell) resolves. No-op pass.\n"
        );
        return;
    };

    // SKIP too when no parsers tree resolves: injection needs a real go `.so` on rtp,
    // which comes from the Bazel-compiled tree or the devShell's nix-built one.
    let Some(parsers_dir) = common::parsers_dir_or_none() else {
        eprintln!(
            "\nSKIP: wolf-ffi treesitter_highlight — no treesitter parsers tree: neither \
             WOLF_TEST_PARSERS_DIR (Bazel) nor WOLF_PARSERS_DIR (Linux devShell) resolves. \
             No-op pass.\n"
        );
        return;
    };

    // Stage the bundle layout in a temp dir T (nvim's working dir + search root,
    // which must outlive the session; kept alive until after `shutdown()`).
    let tmp = tempfile::tempdir().expect("failed to create temp staging dir");
    let root = tmp.path();

    // init.lua next to sibling nvim-treesitter/ + parsers/ — the exact
    // Contents/Resources/ layout the captive block self-locates and gates on.
    let staged_init = common::init_lua_path();
    let init_dst = root.join("init.lua");
    std::fs::copy(&staged_init, &init_dst).expect("failed to copy staged init.lua into temp dir");

    // The staged nvim-treesitter TreeArtifact's inner content is `nvim-treesitter/`;
    // be defensive in case the resolved dir IS already that inner dir.
    let nt_src = {
        let inner = nt_dir.join("nvim-treesitter");
        if inner.is_dir() {
            inner
        } else {
            nt_dir.clone()
        }
    };
    let nt_dst = root.join("nvim-treesitter");
    common::copy_dir_recursive(&nt_src, &nt_dst)
        .expect("failed to recursively copy the nvim-treesitter tree into the temp dir");

    // The staged parsers TreeArtifact's inner content is `parsers/` (holding
    // `parser/<lang>.so`); be defensive in case the resolved dir IS that inner dir.
    let parsers_src = {
        let inner = parsers_dir.join("parsers");
        if inner.is_dir() {
            inner
        } else {
            parsers_dir.clone()
        }
    };
    let parsers_dst = root.join("parsers");
    common::copy_dir_recursive(&parsers_src, &parsers_dst)
        .expect("failed to recursively copy the parsers tree into the temp dir");

    // Catch mis-staging before paying for an nvim launch: go.so and the markdown injections query
    // must resolve under the `T/parsers` and `T/nvim-treesitter` that rtp prepends.
    let go_so = parsers_dst.join("parser").join("go.so");
    assert!(
        go_so.is_file(),
        "staged compiled go parser should exist at {go_so:?} before launch \
         (rtp resolves it as <T/parsers>/parser/go.so)"
    );
    let injections = nt_dst
        .join("queries")
        .join("markdown")
        .join("injections.scm");
    assert!(
        injections.is_file(),
        "staged markdown injections query should exist at {injections:?} before launch \
         (rtp resolves it as <T/nvim-treesitter>/queries/markdown/injections.scm)"
    );

    // A markdown doc with a fenced go block: opening it starts treesitter, and the injections
    // query routes the fence to the go parser.
    let doc_md = root.join("doc.md");
    let doc_contents = "# Heading\n\n```go\npackage main\nfunc f() {}\n```\n";
    std::fs::write(&doc_md, doc_contents).expect("failed to write the markdown doc into temp dir");

    // Argument order mirrors the sibling tests exactly: (init_lua, cwd, cols, rows).
    let handle = wolf_ffi_start(
        init_dst.to_string_lossy().into_owned(),
        root.to_string_lossy().into_owned(),
        common::DEFAULT_COLS,
        common::DEFAULT_ROWS,
    )
    .expect("wolf_ffi_start should return a handle (is WOLF_NVIM_BIN a valid nvim?)");

    // --- Poll read_frame() until attached (cols > 0) ------------------------
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

    // --- Open the markdown buffer to trigger FileType=markdown → treesitter.start
    // Tempdir paths hold no spaces or double quotes, so they are interpolated verbatim into Ex and Lua.
    let doc_md_str = doc_md.to_string_lossy().into_owned();
    send_ex_command(&handle, &format!(":e {doc_md_str}"));

    // --- Sentinel: the highlighter is active, and a full parse yields an injected `go` langtree.
    // See notes/crates/wolf-ffi/tests/treesitter_highlight.rs.md#injection-sentinel
    let sentinel = root.join("treesitter_highlight_proof.txt");
    let sentinel_str = sentinel.to_string_lossy().into_owned();
    let lua_cmd = format!(
        ":lua local b=vim.api.nvim_get_current_buf(); \
         local l1=(vim.treesitter.highlighter.active[b]~=nil) and \"OK\" or \"NO\"; \
         local l2=\"NO\"; \
         local ok,p=pcall(vim.treesitter.get_parser, b); \
         if ok and p then pcall(function() p:parse(true); local kids=p:children(); \
         if kids[\"go\"]~=nil then l2=\"OK\" \
         else for _,child in pairs(kids) do if child:lang()==\"go\" then l2=\"OK\" end end end end) end; \
         vim.fn.writefile({{l1, l2}}, \"{sentinel_str}\")"
    );
    send_ex_command(&handle, &lua_cmd);

    // --- Poll for the sentinel file, then assert both markers ---------------
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut contents = String::new();
    while Instant::now() < deadline {
        if let Ok(s) = std::fs::read_to_string(&sentinel) {
            if s.lines().count() >= EXPECTED_SENTINEL_LINES {
                contents = s;
                break;
            }
        }
        sleep(POLL_INTERVAL);
    }
    assert!(
        contents.lines().count() >= EXPECTED_SENTINEL_LINES,
        "nvim should have written the treesitter-highlight sentinel with \
         {EXPECTED_SENTINEL_LINES} lines within the timeout (got {contents:?})"
    );

    let mut lines = contents.lines();
    let highlighter_active = lines.next().unwrap_or("");
    let go_injected = lines.next().unwrap_or("");
    assert_eq!(
        highlighter_active, MARK_OK,
        "PRECONDITION: treesitter should be attached to the markdown buffer \
         (vim.treesitter.highlighter.active[buf] ~= nil), which the pinned nvim does \
         BY ITSELF whether or not the vendored gate fires (sentinel line 1 = \
         {highlighter_active:?}). This line cannot witness the gate — line 2 is the \
         discriminator. NO here means treesitter did not attach to a markdown buffer \
         at all, so investigate the nvim build or the query loader, not the vendoring"
    );
    assert_eq!(
        go_injected, MARK_OK,
        "the fenced ```go``` block should produce an INJECTED go langtree under the \
         markdown parser (nvim-treesitter markdown injections.scm on rtp routing to \
         the embedded go parser) after a full parse (sentinel line 2 = {go_injected:?})"
    );

    // --- Shutdown: clean teardown -------------------------------------------
    handle.shutdown();
    drop(tmp);
}

// -- Resolver: sibling first, then environment, vanished path dropped; see runtime/nvim/README.md
// The test above stages real siblings, so it would stay green under any reordering.

/// The sibling directory name `init.lua` probes for the queries provider. Fixed
/// by the resolver's first argument, not by this test.
const SIBLING_QUERIES_DIR: &str = "nvim-treesitter";

/// The sibling directory name `init.lua` probes for the compiled parsers.
const SIBLING_PARSERS_DIR: &str = "parsers";

/// The environment variable the queries probe falls back to.
const QUERIES_ENV_VAR: &str = "WOLF_NVIM_TREESITTER_DIR";

/// The environment variable the parsers probe falls back to.
const PARSERS_ENV_VAR: &str = "WOLF_PARSERS_DIR";

/// The augroup `init.lua` creates INSIDE the gate, and therefore the observable
/// for "the gate fired at all".
const GATE_AUGROUP: &str = "WolfTreesitter";

/// Test-local: carries the probe script's absolute path to the child. Read by the
/// single startup `-c` below and by nothing else in the repo — passing it through
/// the environment keeps every path out of the Ex command line, so a path with a
/// space or a quote could not corrupt the command.
const PROBE_LUA_ENV_VAR: &str = "WOLF_GATE_PROBE_LUA";

/// Test-local: carries the sentinel's absolute path to the child, for the same
/// reason, and so the readback does not depend on nvim's working directory.
const PROBE_OUT_ENV_VAR: &str = "WOLF_GATE_PROBE_OUT";

/// Written to sentinel line 4 when a `FileType`/`markdown` autocmd belongs to
/// [`GATE_AUGROUP`] — i.e. the gate fired.
const MARK_AUGROUP_PRESENT: &str = "AUGROUP_PRESENT";

/// Written to sentinel line 4 when no such autocmd exists — the gate no-opped.
const MARK_AUGROUP_ABSENT: &str = "AUGROUP_ABSENT";

/// Written for an unset variable, so an empty line can never be mistaken for one.
const MARK_ENV_UNSET: &str = "<unset>";

/// A module-level global `init.lua` defines WELL AFTER the gate, so its presence
/// witnesses that the file was sourced past the code under test. Without this,
/// an `init.lua` that died on line one would produce the same clean runtimepath
/// and missing augroup that case (b) asserts as a success.
const INIT_RAN_GLOBAL: &str = "wolf_highlight_code_blocks";

/// Written to sentinel line 5 when [`INIT_RAN_GLOBAL`] is defined.
const MARK_INIT_COMPLETE: &str = "INIT_COMPLETE";

/// Written to sentinel line 5 when it is not.
const MARK_INIT_INCOMPLETE: &str = "INIT_INCOMPLETE";

/// Lines the gate probe writes: runtimepath, both environment values as the CHILD
/// saw them, the augroup marker, and the sourced-to-the-end marker.
const GATE_SENTINEL_LINES: usize = 5;

/// The probe script's name inside each case's temp root.
const PROBE_LUA_FILE: &str = "gate-probe.lua";

/// The sentinel's name inside each case's temp root.
const PROBE_OUT_FILE: &str = "gate-probe-out.txt";

/// What the gate left behind in one child nvim, plus that child's captured output
/// so a failure message can show what nvim actually said.
struct GateProbe {
    /// `&runtimepath`, split on commas, trimmed of whitespace and of a trailing
    /// separator so a cosmetic slash cannot defeat a comparison.
    runtimepath: Vec<String>,
    /// `vim.env.WOLF_NVIM_TREESITTER_DIR` as the CHILD saw it.
    queries_env: String,
    /// `vim.env.WOLF_PARSERS_DIR` as the CHILD saw it.
    parsers_env: String,
    /// [`MARK_AUGROUP_PRESENT`] or [`MARK_AUGROUP_ABSENT`].
    augroup: String,
    stdout: String,
    stderr: String,
}

/// Splits nvim's `runtimepath` into comparable entries. Entries are compared for
/// EQUALITY rather than by substring, so no fixture path can satisfy an assertion
/// by being a prefix or suffix of another.
///
/// CONTRACT: the temp roots below must contain no comma and no space — nvim
/// separates the option's entries with commas and backslash-escapes spaces inside
/// them, either of which would break this split. A violation shows up as a
/// positive assertion failing with the whole runtimepath printed, never as a
/// silent pass.
fn runtimepath_entries(rtp: &str) -> Vec<String> {
    rtp.split(',')
        .map(|entry| entry.trim().trim_end_matches('/').to_string())
        .filter(|entry| !entry.is_empty())
        .collect()
}

/// The comparable form of a path, matching [`runtimepath_entries`].
fn rtp_key(path: &Path) -> String {
    path.to_string_lossy().trim_end_matches('/').to_string()
}

/// Runs one pinned nvim over the `init.lua` staged at `root/init.lua`, with
/// exactly `queries_env` and `parsers_env` in the two fallback variables, and
/// reads the gate's leavings back out of a sentinel file.
///
/// The flags mirror `crates/wolf-core/src/nvim.rs`'s `spawn` — `--clean -u <init>
/// -i NONE`, `NVIM_APPNAME` and the four `XDG_*` pinned away from the developer's
/// own — with `--headless` in place of `--embed` and no `env_clear`, so what is
/// exercised is the same startup the application performs.
fn probe_vendored_gate(
    nvim_bin: &Path,
    root: &Path,
    queries_env: &Path,
    parsers_env: &Path,
) -> GateProbe {
    let init = root.join("init.lua");
    assert!(
        init.is_file(),
        "the caller must stage a copy of the captive init.lua at {} before probing",
        init.display()
    );

    let state_root = root.join("nvim-state");
    let config_home = state_root.join("config");
    let data_home = state_root.join("data");
    let state_home = state_root.join("state");
    let cache_home = state_root.join("cache");
    let swap_dir = root.join("nvim-swap");
    for dir in [
        &config_home,
        &data_home,
        &state_home,
        &cache_home,
        &swap_dir,
    ] {
        std::fs::create_dir_all(dir)
            .unwrap_or_else(|e| panic!("failed to create the probe dir {}: {e}", dir.display()));
    }

    // A probe file, not an inline `:lua` line, because the augroup check needs a loop. Every name it
    // interpolates is a constant above, so the Lua cannot drift from the assertions.
    let probe_lua = format!(
        "-- Written by crates/wolf-ffi/tests/treesitter_highlight.rs. Reads back the\n\
         -- state runtime/nvim/init.lua's vendored-tree gate left behind.\n\
         local out = {{}}\n\
         out[1] = vim.o.runtimepath\n\
         out[2] = vim.env.{QUERIES_ENV_VAR} or \"{MARK_ENV_UNSET}\"\n\
         out[3] = vim.env.{PARSERS_ENV_VAR} or \"{MARK_ENV_UNSET}\"\n\
         out[4] = \"{MARK_AUGROUP_ABSENT}\"\n\
         local ok, list = pcall(vim.api.nvim_get_autocmds, \
         {{ event = \"FileType\", pattern = \"markdown\" }})\n\
         if ok and list then\n\
         for _, au in ipairs(list) do\n\
         if au.group_name == \"{GATE_AUGROUP}\" then out[4] = \"{MARK_AUGROUP_PRESENT}\" end\n\
         end\n\
         end\n\
         out[5] = _G.{INIT_RAN_GLOBAL} ~= nil and \"{MARK_INIT_COMPLETE}\" \
         or \"{MARK_INIT_INCOMPLETE}\"\n\
         vim.fn.writefile(out, vim.env.{PROBE_OUT_ENV_VAR})\n"
    );
    let probe_path = root.join(PROBE_LUA_FILE);
    std::fs::write(&probe_path, &probe_lua).unwrap_or_else(|e| {
        panic!(
            "failed to write the gate probe {}: {e}",
            probe_path.display()
        )
    });

    let sentinel = root.join(PROBE_OUT_FILE);
    let _ = std::fs::remove_file(&sentinel);

    let mut child = Command::new(nvim_bin)
        .arg("--headless")
        .arg("--clean")
        .arg("-u")
        .arg(&init)
        .arg("-i")
        .arg("NONE")
        .arg("-c")
        .arg(format!("lua dofile(vim.env.{PROBE_LUA_ENV_VAR})"))
        .arg("-c")
        .arg("qa!")
        .current_dir(root)
        .env("NVIM_APPNAME", "wolf-gate-probe")
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_DATA_HOME", &data_home)
        .env("XDG_STATE_HOME", &state_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .env("WOLF_SWAP_DIR", &swap_dir)
        .env(QUERIES_ENV_VAR, queries_env)
        .env(PARSERS_ENV_VAR, parsers_env)
        .env(PROBE_LUA_ENV_VAR, &probe_path)
        .env(PROBE_OUT_ENV_VAR, &sentinel)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| {
            panic!(
                "failed to spawn the pinned nvim at {} for the gate probe: {e}",
                nvim_bin.display()
            )
        });

    let deadline = Instant::now() + FLUSH_TIMEOUT;
    let mut exited = false;
    while Instant::now() < deadline {
        if child
            .try_wait()
            .expect("failed to poll the gate-probe nvim child")
            .is_some()
        {
            exited = true;
            break;
        }
        sleep(POLL_INTERVAL);
    }
    if !exited {
        let _ = child.kill();
    }
    let status = child
        .wait()
        .expect("failed to reap the gate-probe nvim child");

    // Safe to read only now: the child has exited (or been killed), so draining
    // the pipes cannot deadlock against a full buffer.
    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut stdout);
    }
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut stderr);
    }

    assert!(
        exited,
        "the gate-probe nvim did not exit within {FLUSH_TIMEOUT:?} and was killed. \
         Both startup commands are `-c` arguments, so it should have quit on its \
         own; a hang means something in init.lua blocked on a prompt under \
         `--headless`.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        status.success(),
        "the gate-probe nvim must exit cleanly, got {status}. This is asserted \
         BEFORE the sentinel because a crashed nvim and an nvim that simply wrote \
         nothing leave the same empty directory behind.\nstdout: {stdout}\n\
         stderr: {stderr}"
    );

    let contents = std::fs::read_to_string(&sentinel).unwrap_or_else(|e| {
        panic!(
            "nvim exited cleanly but wrote no gate-probe sentinel at {} ({e}). The \
             probe script runs as a startup `-c`, so this means the script itself \
             failed rather than that the gate answered anything.\nstdout: {stdout}\n\
             stderr: {stderr}",
            sentinel.display()
        )
    });
    let lines: Vec<&str> = contents.lines().collect();
    assert!(
        lines.len() >= GATE_SENTINEL_LINES,
        "the gate-probe sentinel must carry {GATE_SENTINEL_LINES} lines \
         (runtimepath, {QUERIES_ENV_VAR}, {PARSERS_ENV_VAR}, the augroup marker, \
         the sourced-to-the-end marker); got {contents:?}.\nstdout: {stdout}\n\
         stderr: {stderr}"
    );
    assert_eq!(
        lines[4], MARK_INIT_COMPLETE,
        "the staged init.lua must have been sourced PAST the gate, witnessed by \
         `_G.{INIT_RAN_GLOBAL}` (defined at module level well below it). This is a \
         precondition, not a result: an init.lua that raised before the gate would \
         leave exactly the clean runtimepath and missing `{GATE_AUGROUP}` augroup \
         that the stale-path case asserts as its SUCCESS, so without this check \
         that case could pass while measuring nothing.\nstdout: {stdout}\n\
         stderr: {stderr}"
    );

    GateProbe {
        runtimepath: runtimepath_entries(lines[0]),
        queries_env: lines[1].to_string(),
        parsers_env: lines[2].to_string(),
        augroup: lines[3].to_string(),
        stdout,
        stderr,
    }
}

/// Pins `resolve_vendored_dir`'s resolution ORDER, in the two states no other
/// test puts it in.
///
/// Case (a): both siblings staged AND both environment variables pointing at
/// existing, DIFFERENT directories. The siblings must be what lands on
/// runtimepath and the environment paths must not appear at all. The falsifying
/// edit is swapping the two branches of `resolve_vendored_dir` so the environment
/// is consulted first.
///
/// Case (b): no siblings, and both variables pointing at a path that does not
/// exist — a store path collected under a long-lived shell. The gate must not
/// fire: nothing goes on runtimepath and the augroup is absent. The falsifying
/// edit is dropping the step-2 `uv.fs_stat`, which would return the dead path and
/// fire the gate around it.
///
/// Both cases assert what the CHILD saw in the two variables, so neither can pass
/// because the environment failed to reach it.
#[test]
fn vendored_dirs_resolve_sibling_first_and_a_stale_env_path_is_a_no_op() {
    // The only skip. Not gated on the vendored trees: every directory below is a decoy this test
    // creates, so it runs on the cargo/Linux path too.
    let Some(nvim_bin) = common::nvim_bin_or_skip() else {
        return;
    };

    // --- (a) sibling wins over an environment that also resolves.
    // Canonicalize the root first so init.lua's `:p:h` report matches it (macOS `/var` symlink).
    let sibling_case = tempfile::tempdir().expect("failed to create the sibling-case temp dir");
    let sibling_root = sibling_case
        .path()
        .canonicalize()
        .expect("failed to canonicalize the sibling-case temp dir");
    let root = sibling_root.as_path();
    let staged_init = common::init_lua_path();
    std::fs::copy(&staged_init, root.join("init.lua"))
        .expect("failed to copy the captive init.lua into the sibling-case temp dir");

    // Siblings carry the probed names; the environment decoys sit in their own subdirectory, so an
    // assertion about one can never be satisfied by the other's path.
    let sibling_queries = root.join(SIBLING_QUERIES_DIR);
    let sibling_parsers = root.join(SIBLING_PARSERS_DIR);
    let env_queries = root.join("env-side").join("queries-tree");
    let env_parsers = root.join("env-side").join("parsers-tree");
    for dir in [
        &sibling_queries,
        &sibling_parsers,
        &env_queries,
        &env_parsers,
    ] {
        std::fs::create_dir_all(dir)
            .unwrap_or_else(|e| panic!("failed to create the decoy dir {}: {e}", dir.display()));
    }

    let probe = probe_vendored_gate(&nvim_bin, root, &env_queries, &env_parsers);

    let env_queries_str = env_queries.to_string_lossy().into_owned();
    let env_parsers_str = env_parsers.to_string_lossy().into_owned();
    assert_eq!(
        probe.queries_env, env_queries_str,
        "PREMISE, measured rather than assumed: the child must actually SEE a \
         competing {QUERIES_ENV_VAR}. If the environment never reached it, \
         `the sibling wins` would be vacuously true — there would have been \
         nothing for the sibling to win against, and this whole case would assert \
         nothing.\nstderr: {}",
        probe.stderr
    );
    assert_eq!(
        probe.parsers_env, env_parsers_str,
        "PREMISE, measured rather than assumed: the child must actually SEE a \
         competing {PARSERS_ENV_VAR}, for the reason given for the queries \
         variable above.\nstderr: {}",
        probe.stderr
    );

    let sibling_queries_key = rtp_key(&sibling_queries);
    let sibling_parsers_key = rtp_key(&sibling_parsers);
    let env_queries_key = rtp_key(&env_queries);
    let env_parsers_key = rtp_key(&env_parsers);

    assert!(
        probe.runtimepath.contains(&sibling_queries_key),
        "with a `{SIBLING_QUERIES_DIR}/` sibling next to init.lua, THAT directory \
         is what `resolve_vendored_dir` must return and therefore what is \
         prepended to runtimepath. Its absence means either that the gate did not \
         fire at all, or that the resolver answered with something else — the \
         next assertion separates those two. runtimepath = {:?}",
        probe.runtimepath
    );
    assert!(
        probe.runtimepath.contains(&sibling_parsers_key),
        "the inner `{SIBLING_PARSERS_DIR}/` probe resolves sibling-first for the \
         same reason as the outer one, so the sibling must be on runtimepath. \
         runtimepath = {:?}",
        probe.runtimepath
    );
    assert!(
        !probe.runtimepath.contains(&env_queries_key),
        "SIBLING FIRST, environment second: {QUERIES_ENV_VAR} pointed at a \
         directory that EXISTS, and it must still have been ignored, because the \
         sibling resolved at branch 1 and `vim.env` was never read. Finding it \
         here means the two branches of `resolve_vendored_dir` were reordered — \
         and that reordering is exactly what would let a developer's devShell \
         exports replace a signed macOS bundle's own Mach-O parsers with a Linux \
         ELF tree, a mis-load with no symptom until a dlopen fails inside a \
         pcall. It is also what makes `macOS behaviour is unchanged` a \
         construction rather than a hope, on a host that cannot run the macOS \
         side. runtimepath = {:?}",
        probe.runtimepath
    );
    assert!(
        !probe.runtimepath.contains(&env_parsers_key),
        "SIBLING FIRST for the inner probe too: {PARSERS_ENV_VAR} pointed at an \
         existing directory and must have been ignored. This one is the more \
         damaging half to get wrong — the parsers are the platform-specific \
         artifact, so an environment-first resolution loads ELF objects into a \
         Mach-O bundle. runtimepath = {:?}",
        probe.runtimepath
    );
    assert_eq!(
        probe.augroup, MARK_AUGROUP_PRESENT,
        "the gate must have FIRED in this case: a `{SIBLING_QUERIES_DIR}/` \
         sibling resolves, so init.lua installs its FileType=markdown autocmd in \
         the `{GATE_AUGROUP}` group. This is also the control for case (b) below: \
         the same probe must be able to answer {MARK_AUGROUP_PRESENT}, or that \
         case's {MARK_AUGROUP_ABSENT} would prove nothing.\nstdout: {}\nstderr: {}",
        probe.stdout, probe.stderr
    );

    drop(sibling_case);

    // --- (b) a stale environment path degrades to the bare-checkout no-op ----
    // Canonicalized for the reason given in case (a).
    let stale_case = tempfile::tempdir().expect("failed to create the stale-case temp dir");
    let stale_root = stale_case
        .path()
        .canonicalize()
        .expect("failed to canonicalize the stale-case temp dir");
    let root = stale_root.as_path();
    std::fs::copy(&staged_init, root.join("init.lua"))
        .expect("failed to copy the captive init.lua into the stale-case temp dir");

    // Never created: this is the `nix store gc` under a long-lived shell case,
    // where the variable is still exported and its target is gone.
    let collected = root.join("collected-store-path");
    let stale_queries = collected.join("queries-tree");
    let stale_parsers = collected.join("parsers-tree");
    assert!(
        !stale_queries.exists() && !stale_parsers.exists(),
        "the stale case is only the stale case while its two paths do not exist: \
         {} / {}",
        stale_queries.display(),
        stale_parsers.display()
    );

    let probe = probe_vendored_gate(&nvim_bin, root, &stale_queries, &stale_parsers);

    let stale_queries_str = stale_queries.to_string_lossy().into_owned();
    let stale_parsers_str = stale_parsers.to_string_lossy().into_owned();
    assert_eq!(
        probe.queries_env, stale_queries_str,
        "PREMISE, measured rather than assumed: the child must have SEEN the \
         stale {QUERIES_ENV_VAR}. An unset variable would also produce a no-op — \
         that is the bare-checkout state, not the case under test, and the two are \
         indistinguishable from runtimepath alone.\nstderr: {}",
        probe.stderr
    );
    assert_eq!(
        probe.parsers_env, stale_parsers_str,
        "PREMISE, measured rather than assumed: the child must have SEEN the \
         stale {PARSERS_ENV_VAR}, for the reason given for the queries variable \
         above.\nstderr: {}",
        probe.stderr
    );

    let stale_queries_key = rtp_key(&stale_queries);
    let stale_parsers_key = rtp_key(&stale_parsers);
    assert!(
        !probe.runtimepath.contains(&stale_queries_key),
        "a resolved path that has VANISHED must fall through to nil, not onto \
         runtimepath: that is what the step-2 `uv.fs_stat` in \
         `resolve_vendored_dir` is for. Finding it here means the resolver \
         returned a dead directory, so nvim would search a path that does not \
         exist for every query it loads. runtimepath = {:?}",
        probe.runtimepath
    );
    assert!(
        !probe.runtimepath.contains(&stale_parsers_key),
        "the inner probe must fall through to nil on a vanished path too. \
         runtimepath = {:?}",
        probe.runtimepath
    );
    let queries_needle = format!("/{SIBLING_QUERIES_DIR}");
    let parsers_needle = format!("/{SIBLING_PARSERS_DIR}");
    let stray_sibling_entry = probe
        .runtimepath
        .iter()
        .find(|entry| entry.ends_with(&queries_needle) || entry.ends_with(&parsers_needle));
    assert!(
        stray_sibling_entry.is_none(),
        "with no siblings staged and both variables dead, NOTHING may be \
         prepended for the vendored trees — this catches a resolver that fell \
         back to some other directory rather than to nil. Found {:?} in \
         runtimepath = {:?}",
        stray_sibling_entry,
        probe.runtimepath
    );
    assert_eq!(
        probe.augroup, MARK_AUGROUP_ABSENT,
        "the gate must NOT have fired: with the queries probe answering nil, \
         init.lua must not install its FileType=markdown autocmd in the \
         `{GATE_AUGROUP}` group. A {MARK_AUGROUP_PRESENT} here means the gate \
         half-fired around a dead path — the outcome the step-2 `fs_stat` exists \
         to prevent, and one that leaves `vim.treesitter.start` running against \
         queries that are not there.\nstdout: {}\nstderr: {}",
        probe.stdout, probe.stderr
    );

    drop(stale_case);
}
