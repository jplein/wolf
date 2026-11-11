//! The terminfo smoke: a terminal pane opened by a typed `:terminal` runs its shell with
//! `TERM=xterm-ghostty` and `TERMINFO` naming the directory `WOLF_TERMINFO_DIR` gives the app.
//! Its shell, scenario and readback are shared with the packaged smoke in `packaged.rs`.

use super::*;

/// The devShell's export of the compiled terminfo directory, from `wolfGhosttyVt`.
const TEST_TERMINFO_ENV: &str = "WOLF_TEST_TERMINFO_DIR";

/// The variable the app reads Wolf's terminfo directory from, which the package wrapper
/// defaults and this smoke sets for the checkout binary.
pub(crate) const TERMINFO_DIR_ENV: &str = "WOLF_TERMINFO_DIR";

/// The terminal type Wolf's own entry defines.
pub(crate) const GHOSTTY_TERM: &str = "xterm-ghostty";

/// The entry in ncurses' letter layout, which nixpkgs' `tic` writes.
pub(crate) const LETTER_LAYOUT_ENTRY: &str = "x/xterm-ghostty";

/// A `TERM` the app is given that no session should pass on: the shell showing it inherited it.
pub(crate) const DECOY_TERM: &str = "dumb";

/// A `TERMINFO` the app is given that names no database: the shell showing it inherited it.
pub(crate) const DECOY_TERMINFO: &str = "/nonexistent/wolf-decoy-terminfo";

/// The shell's readback, under `XDG_RUNTIME_DIR` so no sidebar row draws it.
const TERMINFO_READBACK_NAME: &str = "wolf_gnome_terminfo.txt";

/// `infocmp`'s whole output, beside the readback, kept as evidence.
const TERMINFO_INFOCMP_NAME: &str = "wolf_gnome_terminfo_infocmp.txt";

/// The script the app runs as `SHELL`, under `XDG_RUNTIME_DIR`.
const TERMINFO_SHELL_NAME: &str = "wolf_gnome_terminfo_shell.sh";

/// The `SHELL` a typed `:terminal` runs: it writes `$TERM`, `$TERMINFO`, the exit status of
/// `infocmp -x xterm-ghostty` and whether its output names `setrgbf` into `$TI_READBACK`,
/// through a rename so a poll never reads half of it, and `infocmp`'s output into
/// `$TI_INFOCMP`. Then it sleeps, so the session stays open until the app goes.
const TERMINFO_SHELL: &str = r#"#!/bin/sh
out=$(infocmp -x xterm-ghostty 2>&1)
rc=$?
case "$out" in *setrgbf=*) s=yes ;; *) s=no ;; esac
printf '%s\n' "$out" > "$TI_INFOCMP"
printf '%s\n%s\n%s\n%s\n' "$TERM" "$TERMINFO" "$rc" "$s" > "$TI_READBACK.tmp"
mv "$TI_READBACK.tmp" "$TI_READBACK"
exec sleep 600
"#;

/// The TERMINFO scenario, run as a single `sh -c` string inside `cage`: launch, type
/// `:terminal` so Neovim's current window opens a session on `SHELL`, and wait for the
/// shell's readback.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `40` — it never became ready (the shared convention).
/// * `31` — the shell's readback never appeared: no session opened, or its shell never ran.
pub(crate) const TERMINFO_SCENARIO: &str = r#"
set -eu

WAIT_INTERVAL=0.2
FILE_TRIES=100

cleanup() {
    set +e
    if [ -n "${app_pid:-}" ]; then
        kill "$app_pid" 2>/dev/null
        wait "$app_pid" 2>/dev/null
    fi
}
trap cleanup EXIT INT TERM HUP

wait_file() {
    f=$1
    t=0
    while [ "$t" -lt "$FILE_TRIES" ]; do
        if [ -s "$f" ]; then
            return 0
        fi
        t=$((t + 1))
        sleep "$WAIT_INTERVAL"
    done
    echo "wolf-gnome terminfo smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome terminfo smoke"

wtype -- ":terminal"
wtype -k Return
wait_file "$TI_READBACK" || exit 31

exit 0
"#;

/// What the shell wrote, line by line.
#[derive(Debug)]
pub(crate) struct TerminfoReadback {
    pub(crate) term: String,
    pub(crate) terminfo: String,
    pub(crate) infocmp_status: String,
    pub(crate) setrgbf: String,
}

/// The paths of one terminfo smoke's script and evidence, cleared of any earlier run's.
pub(crate) struct TerminfoFiles {
    pub(crate) shell: PathBuf,
    pub(crate) readback: PathBuf,
    pub(crate) infocmp: PathBuf,
}

impl TerminfoFiles {
    /// Writes [`TERMINFO_SHELL`] as an executable under `runtime`, and clears the evidence.
    pub(crate) fn prepare(runtime: &Path, smoke: &str) -> Self {
        use std::os::unix::fs::PermissionsExt as _;

        let files = Self {
            shell: runtime.join(TERMINFO_SHELL_NAME),
            readback: runtime.join(TERMINFO_READBACK_NAME),
            infocmp: runtime.join(TERMINFO_INFOCMP_NAME),
        };
        for path in [&files.readback, &files.infocmp] {
            clear_evidence(path, smoke);
        }
        std::fs::write(&files.shell, TERMINFO_SHELL).unwrap_or_else(|e| {
            panic!(
                "{smoke}: failed to write the shell {}: {e}",
                files.shell.display()
            )
        });
        std::fs::set_permissions(&files.shell, std::fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|e| {
                panic!(
                    "{smoke}: failed to make {} executable: {e}",
                    files.shell.display()
                )
            });
        files
    }

    /// Sets `SHELL`, the decoys and the evidence paths on a smoke's `cage` command.
    pub(crate) fn apply<'c>(&self, cmd: &'c mut Command) -> &'c mut Command {
        cmd.env("SHELL", &self.shell)
            .env("TERM", DECOY_TERM)
            .env("TERMINFO", DECOY_TERMINFO)
            .env("TI_READBACK", &self.readback)
            .env("TI_INFOCMP", &self.infocmp)
    }

    /// The four lines the shell wrote. Panics, naming the file, when it holds another count.
    pub(crate) fn read(&self, smoke: &str) -> TerminfoReadback {
        let raw = read_readback(&self.readback, smoke);
        let lines: Vec<&str> = raw.lines().collect();
        let [term, terminfo, infocmp_status, setrgbf] = lines[..] else {
            panic!(
                "{smoke}: the shell's readback {} holds {} lines, not the four \
                 `TERM`, `TERMINFO`, infocmp's status and setrgbf: {raw:?}",
                self.readback.display(),
                lines.len()
            );
        };
        TerminfoReadback {
            term: term.to_owned(),
            terminfo: terminfo.to_owned(),
            infocmp_status: infocmp_status.to_owned(),
            setrgbf: setrgbf.to_owned(),
        }
    }
}

/// **A terminal pane's shell gets Wolf's `xterm-ghostty` entry under cage.**
///
/// The checkout binary runs with `WOLF_TERMINFO_DIR` set to the devShell's compiled entry and
/// with the decoys `TERM=dumb` and `TERMINFO=/nonexistent/…`. A typed `:terminal` opens a
/// session on a `SHELL` script that writes what it sees into a readback file. No capture: the
/// claim is about the shell's environment, which only the shell can report.
///
/// `crates/wolf-core/tests/terminal_terminfo.rs` proves core's choice; this proves that
/// `window.rs` hands core the directory from the variable.
///
/// ## Assertions
///
/// | # | assertion | kind | falsifier |
/// | --- | --- | --- | --- |
/// | 0 | `WOLF_TEST_TERMINFO_DIR` holds the entry in the letter layout | precondition | none |
/// | 1 | the scenario exits 0 | precondition | none |
/// | 2 | the shell's `TERM` is `xterm-ghostty` | discriminator | `window.rs` builds `CoreConfig` with `terminfo_dir: None` |
/// | 3 | the shell's `TERMINFO` is the directory the app was given | discriminator | the same edit, which leaves the decoy |
#[test]
fn terminal_pane_shell_gets_wolfs_terminfo_under_cage() {
    const SMOKE: &str = "terminal_pane_shell_gets_wolfs_terminfo_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // --- Assertion 0 (precondition): the devShell's entry is there to hand over ------
    let terminfo_dir = std::env::var(TEST_TERMINFO_ENV)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| {
            panic!(
                "{SMOKE}: {TEST_TERMINFO_ENV} is unset or empty. The Linux devShell exports it as \
                 ${{wolfGhosttyVt}}/share/terminfo; run this inside the devShell."
            )
        });
    let entry = Path::new(&terminfo_dir).join(LETTER_LAYOUT_ENTRY);
    assert!(
        entry.is_file(),
        "{SMOKE}: {} is not a file, so the devShell's {TEST_TERMINFO_ENV} holds no compiled \
         {GHOSTTY_TERM} entry and the assertions below would fail on the build, not on the app. \
         `wolfGhosttyVt`'s postInstall compiles it with ncurses' `tic -x`.",
        entry.display()
    );

    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[]);
    let files = TerminfoFiles::prepare(&runtime_dir, SMOKE);

    let mut cmd = cage_command(TERMINFO_SCENARIO, &cwd, &runtime_dir);
    files
        .apply(&mut cmd)
        .env("WOLF_BIN", bin)
        .env(TERMINFO_DIR_ENV, &terminfo_dir);
    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless terminfo scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage scenario exited unsuccessfully ({status}). Exit 11: the app died before \
         it was ready for input. Exit 40: it never became ready. Exit 31: the shell's readback \
         never appeared at {}, so no session opened on `:terminal` (cage.log holds a \
         `wolf-gnome:` line if open_terminal failed) or its SHELL {} never ran.",
        files.readback.display(),
        files.shell.display()
    );
    let readback = files.read(SMOKE);

    // --- Assertion 2 (discriminator): the terminal type -----------------------
    assert_eq!(
        readback.term, GHOSTTY_TERM,
        "{SMOKE}: the pane's shell sees TERM={:?}. xterm-256color means the app did not pass \
         {TERMINFO_DIR_ENV} into `CoreConfig.terminfo_dir`, or core did not find the entry in \
         {terminfo_dir}; {DECOY_TERM:?} means TERM was inherited rather than set. Readback: \
         {readback:?}",
        readback.term
    );

    // --- Assertion 3 (discriminator): Wolf's own TERMINFO ---------------------
    assert_eq!(
        readback.terminfo, terminfo_dir,
        "{SMOKE}: the pane's shell sees TERMINFO={:?}, not the {TERMINFO_DIR_ENV} the app was \
         given. {DECOY_TERMINFO:?} means the inherited TERMINFO was not replaced; an empty value \
         means it was removed and Wolf's was not set. Readback: {readback:?}",
        readback.terminfo
    );
}
