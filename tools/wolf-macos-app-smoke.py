"""On-demand smoke for the real Bazel-built ``Wolf.app``: a zoom across quit and relaunch, driven
through the drained Ex-command route.

``tools/wolf-macos-app-smoke.sh`` builds ``//crates/wolf-macos:Wolf`` and hands its ``Wolf.zip``
to this harness as ``--app-zip``. The harness extracts a private copy of the bundle, launches it
twice in the logged-in GUI session against isolated state and config directories, and drives it
only through Neovim: a two-line prelude (``PRELUDE_LUA``) opens a socket before it sources the
bundled ``init.lua``, and the bundle's own ``nvim`` evaluates one expression per call through
``--server <sock> --remote-expr``. It reads back only Neovim window sizes and ``state.json``.

Every Wolf Ex command is an ``rpcnotify`` on core's channel, captured when core calls
``wolf_setup_commands`` after the init file loads, so one issued by a second RPC client takes
the route a typed one does: ``wolf_command`` ->
core's queue -> wake -> ``TabCore.onWake`` -> ``drainAndDispatchCommands`` ->
``CommandDispatcher`` -> ``WolfWindowController``.

Scenario
========

L1 is the ``(w, h)`` of the lone launch pane in run 1. ``W_za`` is that pane's Neovim window and
``W_zb`` the window the split adds.

Run 1 starts from a seeded ``state.json`` holding one window, one tab and a lone ``za.md`` leaf.

1. P0 (premise): the socket answers, ``:WolfZoomPane`` exists, one editor window shows ``za.md``,
   and its size is stable at L1 with ``w > 1`` and ``h > 1``.
2. C3: ``:WolfSplitVertical``.
3. P1 (premise): ``:edit zb.md`` in the current (new) window; it shows ``zb.md``.
4. C1: ``:WolfZoomPane``.
5. Q: ``:WolfQuitApplication``.
6. R1: run 1's ``state.json``, copied to ``logs/run1-state.json`` before run 2 can rewrite it.

Run 2 shares the state and config dirs and gets a new socket.

1. P2 (premise): the socket answers, ``:WolfZoomPane`` exists, and there are two editor windows.
2. R2: the relaunch restored the zoom.
3. C2, then C2z: ``:WolfTerminal``.
4. Q: ``:WolfQuitApplication``.

Assertions
==========

The falsifying edits are in production code only; each should turn its own ID red, first.

| ID  | Observable | Falsifying edit | Predicted red |
| --- | --- | --- | --- |
| P0  | premise: see the scenario | none | n/a |
| C3  | after ``:WolfSplitVertical``: two editor windows, the current one new (``!= W_za``), both ``w < L1.w``, both ``h == L1.h`` | ``CommandDispatcher.dispatch``: the ``case .splitVertical:`` body becomes ``break`` | C3 (one window) |
| P1  | premise: the current window shows ``zb.md`` and ``W_za`` still shows ``za.md`` | none | n/a |
| C1  | after ``:WolfZoomPane``: ``cur == W_zb``, ``w(W_zb) == L1.w``, ``h(W_zb) == L1.h``, and ``W_za`` still listed with ``w < L1.w`` (hidden, kept at its size) | the ``case .zoomPane:`` body becomes ``break`` | C1 (zb about half of L1.w) |
| Q   | each launch exits with status 0 within ``QUIT_TIMEOUT_S`` of ``:WolfQuitApplication`` | the ``case .quitApplication:`` body becomes ``break`` | Q, at its first check in run 1; the harness then kills the app |
| R1  | run 1's ``state.json``: version 1; one window; one tab; tab ``cwd == P``; layout a ``vertical`` split of exactly two leaves, tails ``za.md`` (``zoomed`` absent or false) then ``zb.md`` (``zoomed: true``) | ``WolfWindowController.liveTab()``: drop the ``zoomed: paneContainer.zoomedPaneId`` argument to ``SessionSeam.gatherTab`` (it defaults to ``nil``) | R1 (no leaf zoomed) |
| P2  | premise: see the scenario | none | n/a |
| R2  | run 2: two editor windows, no drawer; the current one shows ``zb.md`` at ``w == L1.w`` and ``h == L1.h``; the other window ``w < L1.w`` | ``PaneContainerView.rebuildLayout(from:node:)``: delete the ``if let index = saved.zoomedLeafIndex ...`` re-zoom block | R2 (zb about half of L1.w) |
| C2  | after ``:WolfTerminal`` in run 2: exactly one drawer window (``w:wolf_drawer``), which is current, with ``0 < h < L1.h`` | the ``case .terminal:`` body becomes ``break`` | C2 (no drawer) |
| C2z | at the same point, after C2: ``w(W_zb) < L1.w`` and ``h(W_zb) < L1.h`` (unzoomed, laid out above the drawer) | ``PaneZoom.zoomed(after:...)``: the ``.split``, ``.drawerOpen``, ``.focusMove`` and ``.toggleTerminal`` arms each ``return zoomed`` unchanged | C2z, with C2 green |

Why each observable discriminates
---------------------------------

- In every check the window whose size discriminates was sized by the front-end, and Neovim's
  own layout does not override a front-end-requested grid size.
- C3 asserts the route through the window count only: Neovim's own ``:vsplit`` gives half widths
  too, so the widths add nothing a broken route would not also show.
- C1 and R2 use zb's width, which equals L1.w only when the front-end zoomed zb.
- C2z tests zb, not za: in run 2 za was never sized by the front-end, so Neovim's own drawer
  split shrinks it whether or not the zoom ended.
- Every wait needs its predicate to hold on ``STABLE_READS`` probes in a row, so a transient
  layout mid-resize passes no check.
- The checks run in sequence, so a falsified ID is the first red. A premise failure reads
  ``FAIL P<n> (premise): ...`` and has no falsifier.

Premises about the dependencies
-------------------------------

Each is measured by a green baseline run against the real components, not assumed:

- ``winwidth()`` and ``winheight()`` report the grid size the front-end requested, the fact the
  GNOME twin's ``Z1 == L1`` rests on. A red C1 or R2 on unchanged code means this is false here.
- ``nvim --clean --headless --server S --remote-expr E`` prints E's value on stdout. If it does
  not, P0 fails with no answer from the socket.
- ``TabCore.swift`` honours ``WOLF_INIT_LUA`` and core passes the ``WOLF_*`` environment through
  to Neovim, so the prelude finds its socket path and the bundled ``init.lua``.

Refusals and leftovers
======================

The harness refuses with exit 2 on a locked screen, re-checked before each launch, and cleanup
removes the AppKit saved state only if the run created it; see
``notes/tools/wolf-macos-app-smoke.sh.md`` and ``notes/tools/wolf-macos-app-smoke.py.md``.

Exit codes
==========

0 all green; 1 an assertion or premise failed; 2 a refusal (including a locked screen), a setup
error or an unexpected harness exception; 128 + the signal number on SIGINT, SIGTERM or SIGHUP.
On any non-zero exit once the run directory exists, its ``logs/``, ``state/``, ``config/``,
``project/`` and ``client/`` are kept and their path printed.
"""

import argparse
import dataclasses
import json
import os
import platform
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import traceback
from dataclasses import dataclass
from typing import Callable, Dict, List, Optional, Tuple

READY_TIMEOUT_S = 60
STEP_TIMEOUT_S = 20
POLL_INTERVAL_S = 0.2
RPC_TIMEOUT_S = 5
QUIT_TIMEOUT_S = 20
TERM_GRACE_S = 5
STABLE_READS = 3
MESSAGES_TIMEOUT_S = 2
LOG_TAIL_LINES = 40

# macOS `sun_path` is 104 bytes, NUL included.
SUN_PATH_MAX = 104

LEFT_FILE = "za.md"
ZOOMED_FILE = "zb.md"
CLIENT_APPNAME = "wolf-app-smoke-client"

# Keep AppKit off the saved state the user's own Wolf shares, and App Nap off the run.
APP_FLAGS = ("-ApplePersistenceIgnoreState", "YES", "-NSAppSleepDisabled", "YES")
# Drop `--headless` if it stops `--remote-expr` printing the value.
CLIENT_FLAGS = ("--clean", "--headless")

# The whole tabpage in one evaluation, so a probe never mixes two layouts.
# See notes/tools/wolf-macos-app-smoke.py.md#driving-neovim
PROBE_EXPR = (
    "json_encode({'cur': win_getid(), 'cmd': exists(':WolfZoomPane'), "
    "'wins': map(filter(nvim_tabpage_list_wins(0), {_, w -> win_gettype(w) ==# ''}), "
    "{_, w -> {'id': w, 'w': winwidth(w), 'h': winheight(w), "
    "'name': fnamemodify(bufname(winbufnr(w)), ':t'), "
    "'drawer': getwinvar(w, 'wolf_drawer', v:false) ? 1 : 0}})})"
)

# A sentinel rather than '', so a client that printed nothing never reads as success.
EX_OK = "wolf-app-smoke:ok"

# `{arg}` is a Vimscript string literal (`vim_string`); values reach Lua only through `_A`.
EX_EXPR = (
    'luaeval("(function(c) local ok, e = pcall(vim.cmd, c) '
    "if ok then return '" + EX_OK + "' end return tostring(e) end)(_A)\", {arg})"
)
EDIT_EXPR = (
    "luaeval(\"(function(p) local ok, e = pcall(vim.cmd, 'edit ' .. vim.fn.fnameescape(p)) "
    "if ok then return '" + EX_OK + "' end return tostring(e) end)(_A)\", {arg})"
)
MESSAGES_EXPR = "execute('messages')"

# serverstart first, so a broken init.lua still leaves a socket; dofile keeps its self-location.
# See notes/tools/wolf-macos-app-smoke.py.md#driving-neovim
PRELUDE_LUA = (
    "pcall(vim.fn.serverstart, vim.env.WOLF_APP_SMOKE_SOCKET)\n"
    "dofile(vim.env.WOLF_APP_SMOKE_BUNDLED_INIT)\n"
)

# macOS base tools by absolute path: the user's PATH may put GNU coreutils first.
IOREG = "/usr/sbin/ioreg"
GETCONF = "/usr/bin/getconf"
DITTO = "/usr/bin/ditto"
PGREP = "/usr/bin/pgrep"

SAVED_STATE_NAME = "org.wolf.WolfMacOS.savedState"
SCREEN_LOCKED_RE = re.compile(r'"(?:CGSSessionScreenIsLocked|IOConsoleLocked)"\s*=\s*Yes')
SCREEN_LOCKED_MESSAGE = "unlock the screen; a window cannot become key while it is locked."

SCRUBBED_ENV_NAMES = ("NVIM", "NVIM_LISTEN_ADDRESS", "VIMINIT", "MYVIMRC", "ENV")
SCRUBBED_ENV_PREFIXES = ("WOLF_", "DYLD_")


class SmokeFailure(Exception):
    """An assertion or a premise failed: exit 1."""

    def __init__(self, check_id: str, message: str, probe: Optional["Probe"] = None):
        super().__init__(f"{check_id}: {message}")
        self.check_id = check_id
        self.message = message
        self.probe = probe
        self.app: Optional["App"] = None
        self.nvim: Optional["Nvim"] = None


class SetupError(Exception):
    """A refusal or a setup step that failed before any check could run: exit 2."""


class Interrupted(BaseException):
    """SIGINT, SIGTERM or SIGHUP, raised from the handler so every `finally` runs."""

    def __init__(self, signum: int):
        super().__init__(f"signal {signum}")
        self.signum = signum


@dataclass
class Win:
    id: int
    w: int
    h: int
    name: str
    drawer: int


@dataclass
class Probe:
    cur: int
    cmd: int
    wins: List[Win]

    def editors(self) -> List[Win]:
        return [w for w in self.wins if not w.drawer]

    def drawers(self) -> List[Win]:
        return [w for w in self.wins if w.drawer]

    def drawer(self) -> Optional[Win]:
        found = self.drawers()
        return found[0] if len(found) == 1 else None

    def win(self, win_id: int) -> Optional[Win]:
        for w in self.wins:
            if w.id == win_id:
                return w
        return None

    def current(self) -> Optional[Win]:
        return self.win(self.cur)

    def to_json(self) -> str:
        return json.dumps(dataclasses.asdict(self), sort_keys=True)

    @staticmethod
    def parse(text: str) -> Optional["Probe"]:
        """The probe `text` encodes, or None when it is not the probe's JSON shape."""
        try:
            raw = json.loads(text)
            wins = [
                Win(
                    id=int(w["id"]),
                    w=int(w["w"]),
                    h=int(w["h"]),
                    name=str(w["name"]),
                    drawer=int(w["drawer"]),
                )
                for w in raw["wins"]
            ]
            return Probe(cur=int(raw["cur"]), cmd=int(raw["cmd"]), wins=wins)
        except (ValueError, TypeError, KeyError):
            return None


@dataclass
class RunDir:
    root: str
    app: str
    project: str
    state: str
    config: str
    init: str
    sock: str
    logs: str
    client: str
    tmp: str


@dataclass
class Bundle:
    binary: str
    nvim_client: str
    bundled_init: str


_transcript_path: Optional[str] = None


def log(*parts: str) -> None:
    """Prints a line and appends it to `logs/transcript.txt` once the run directory exists."""
    line = " ".join(parts)
    # A closed terminal (EIO, EPIPE) must not abort the cleanup that follows.
    try:
        print(line, flush=True)
    except OSError:
        pass
    if _transcript_path is None:
        return
    try:
        with open(_transcript_path, "a", encoding="utf-8") as f:
            f.write(line + "\n")
    except OSError:
        pass


def step(text: str) -> None:
    log(f"==> {text}")


def passed(check_id: str, text: str, probe: Optional[Probe] = None) -> None:
    log(f"PASS {check_id}: {text}")
    if probe is not None:
        log(f"    probe: {probe.to_json()}")


def label(check_id: str) -> str:
    return f"{check_id} (premise)" if check_id.startswith("P") else check_id


def vim_string(value: str) -> str:
    """`value` as a Vimscript single-quoted literal, its `'` doubled."""
    return "'" + value.replace("'", "''") + "'"


def refuse_unless_darwin_outside_regina() -> None:
    if platform.system() != "Darwin":
        raise SetupError(f"macOS only: this launches Wolf.app (uname says {platform.system()}).")
    if "REGINA_AGENT" in os.environ:
        raise SetupError("refusing inside a regina worker (REGINA_AGENT is set).")


def refuse_if_screen_locked() -> None:
    """Raises SetupError while `ioreg -n Root -d1` reports the screen locked.

    An unreadable `ioreg` passes: P0's message names a window that never became key first.
    """
    try:
        out = subprocess.run(
            [IOREG, "-n", "Root", "-d1"],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=RPC_TIMEOUT_S,
            check=False,
        ).stdout.decode("utf-8", "replace")
    except (OSError, subprocess.TimeoutExpired):
        return
    if SCREEN_LOCKED_RE.search(out):
        raise SetupError(SCREEN_LOCKED_MESSAGE)


@dataclass
class SavedState:
    """AppKit's saved-state path for the bundle, and whether it existed before the first launch."""

    path: Optional[str]
    existed: bool


def _darwin_user_temp_dir() -> Optional[str]:
    try:
        value = os.confstr("CS_DARWIN_USER_TEMP_DIR")
        if value:
            return value
    except (ValueError, OSError):
        pass
    try:
        result = subprocess.run(
            [GETCONF, "DARWIN_USER_TEMP_DIR"],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=RPC_TIMEOUT_S,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    value = result.stdout.decode("utf-8", "replace").strip()
    return value if result.returncode == 0 and value else None


def record_saved_state() -> SavedState:
    temp = _darwin_user_temp_dir()
    if temp is None:
        log("WARN: no DARWIN_USER_TEMP_DIR; the bundle's savedState will not be cleaned up")
        return SavedState(path=None, existed=False)
    path = os.path.join(temp, SAVED_STATE_NAME)
    return SavedState(path=path, existed=os.path.lexists(path))


def clean_saved_state(saved: SavedState) -> None:
    if saved.path is None:
        return
    if saved.existed:
        log(f"WARN: {saved.path} existed before the run, so it is left in place")
        return
    if os.path.isdir(saved.path) and not os.path.islink(saved.path):
        _rmtree(saved.path)
        return
    try:
        os.remove(saved.path)
    except FileNotFoundError:
        pass
    except OSError as e:
        log(f"WARN: could not delete {saved.path}: {e}")


def make_run_dir() -> RunDir:
    global _transcript_path
    # realpath because /tmp is a symlink, and the socket path must fit `sun_path`.
    root = os.path.realpath(tempfile.mkdtemp(prefix="wolf-app-smoke.", dir="/tmp"))
    run = RunDir(
        root=root,
        app=os.path.join(root, "app"),
        project=os.path.join(root, "project"),
        state=os.path.join(root, "state"),
        config=os.path.join(root, "config"),
        init=os.path.join(root, "init"),
        sock=os.path.join(root, "sock"),
        logs=os.path.join(root, "logs"),
        client=os.path.join(root, "client"),
        tmp=os.path.join(root, "tmp"),
    )
    for d in (run.project, run.state, run.config, run.init, run.sock, run.logs, run.tmp):
        os.makedirs(d)
    for sub in ("config", "data", "state", "cache", "tmp"):
        os.makedirs(os.path.join(run.client, sub))
    _transcript_path = os.path.join(run.logs, "transcript.txt")
    return run


# A private copy of the zip, never the undeclared `_archive-root/` sibling.
# See notes/tools/wolf-macos-app-smoke.py.md#launching-the-bundle
def extract_bundle(zip_path: str, run: RunDir) -> Bundle:
    try:
        result = subprocess.run(
            [DITTO, "-x", "-k", zip_path, run.app],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
    except OSError as e:
        raise SetupError(f"could not run ditto: {e}") from e
    if result.returncode != 0:
        out = result.stdout.decode("utf-8", "replace").strip()
        raise SetupError(f"ditto -x -k {zip_path} failed ({result.returncode}): {out}")
    app = os.path.join(run.app, "Wolf.app")
    bundle = Bundle(
        binary=os.path.join(app, "Contents", "MacOS", "Wolf"),
        nvim_client=os.path.join(app, "Contents", "Resources", "nvim", "bin", "nvim"),
        bundled_init=os.path.join(app, "Contents", "Resources", "init.lua"),
    )
    for path in (bundle.binary, bundle.nvim_client):
        if not (os.path.isfile(path) and os.access(path, os.X_OK)):
            raise SetupError(f"the extracted bundle has no executable {path}")
    if not os.path.isfile(bundle.bundled_init):
        raise SetupError(f"the extracted bundle has no {bundle.bundled_init}")
    return bundle


# The first launch restores a seeded state.json, so Neovim starts in the project.
# See notes/tools/wolf-macos-app-smoke.py.md#the-seeded-first-launch
def write_fixtures(run: RunDir) -> None:
    with open(os.path.join(run.project, LEFT_FILE), "w", encoding="utf-8") as f:
        f.write("# Left pane\n")
    with open(os.path.join(run.project, ZOOMED_FILE), "w", encoding="utf-8") as f:
        f.write("# Zoomed pane\n")
    # Width 0 is the unset sentinel, so the window keeps the size it is built at.
    seeded = {
        "version": 1,
        "windows": [
            {
                "x": 0,
                "y": 0,
                "width": 0,
                "height": 0,
                "tabs": [
                    {
                        "cwd": run.project,
                        "sidebar_visible": True,
                        "layout": {
                            "type": "leaf",
                            "path": os.path.join(run.project, LEFT_FILE),
                            "preview": False,
                        },
                    }
                ],
            }
        ],
    }
    with open(os.path.join(run.state, "state.json"), "w", encoding="utf-8") as f:
        json.dump(seeded, f, indent=2)
        f.write("\n")
    with open(os.path.join(run.init, "smoke_init.lua"), "w", encoding="utf-8") as f:
        f.write(PRELUDE_LUA)


def scrubbed_environ() -> Dict[str, str]:
    env = dict(os.environ)
    for name in list(env):
        if name in SCRUBBED_ENV_NAMES or name.startswith(SCRUBBED_ENV_PREFIXES):
            del env[name]
    return env


# HOME stays: cfprefsd and saved state do not follow it, and the drawer's login shell reads it.
# See notes/tools/wolf-macos-app-smoke.py.md#isolation
def app_env(run: RunDir, bundle: Bundle, sock: str) -> Dict[str, str]:
    env = scrubbed_environ()
    env.update(
        {
            "WOLF_STATE_DIR": run.state,
            "WOLF_CONFIG_DIR": run.config,
            "WOLF_INIT_LUA": os.path.join(run.init, "smoke_init.lua"),
            "WOLF_APP_SMOKE_SOCKET": sock,
            "WOLF_APP_SMOKE_BUNDLED_INIT": bundle.bundled_init,
            "SHELL": "/bin/sh",
            "TMPDIR": run.tmp + "/",
        }
    )
    return env


def client_env(run: RunDir) -> Dict[str, str]:
    env = scrubbed_environ()
    env["NVIM_APPNAME"] = CLIENT_APPNAME
    env["XDG_CONFIG_HOME"] = os.path.join(run.client, "config")
    env["XDG_DATA_HOME"] = os.path.join(run.client, "data")
    env["XDG_STATE_HOME"] = os.path.join(run.client, "state")
    env["XDG_CACHE_HOME"] = os.path.join(run.client, "cache")
    # The client's automatic server socket lands under TMPDIR, so keep it in the run.
    env["TMPDIR"] = os.path.join(run.client, "tmp") + "/"
    return env


class App:
    """One launch of the extracted bundle, leading a process group of its own."""

    def __init__(self, n: int, proc: subprocess.Popen, sock: str, log_path: str, log_file):
        self.n = n
        self.proc = proc
        self.sock = sock
        self.log_path = log_path
        self._log_file = log_file

    # A direct exec, not `open -n`: a PID and a process group, and AppKit's saved state kept off.
    # See notes/tools/wolf-macos-app-smoke.py.md#launching-the-bundle
    @classmethod
    def launch(cls, run: RunDir, bundle: Bundle, n: int) -> "App":
        refuse_if_screen_locked()
        sock = os.path.join(run.sock, f"nvim-{n}.sock")
        if len(os.fsencode(sock)) >= SUN_PATH_MAX:
            raise SetupError(f"socket path {sock} is not under {SUN_PATH_MAX} bytes")
        log_path = os.path.join(run.logs, f"app-run{n}.log")
        log_file = open(log_path, "wb")
        try:
            proc = subprocess.Popen(
                [bundle.binary] + list(APP_FLAGS),
                env=app_env(run, bundle, sock),
                cwd=run.project,
                stdin=subprocess.DEVNULL,
                stdout=log_file,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
        except OSError as e:
            log_file.close()
            raise SetupError(f"could not launch {bundle.binary}: {e}") from e
        log(f"    run {n}: launched pid {proc.pid}, socket {sock}, log {log_path}")
        return cls(n, proc, sock, log_path, log_file)

    @property
    def pgid(self) -> int:
        return self.proc.pid

    def alive(self) -> bool:
        return self.proc.poll() is None

    def group_alive(self) -> bool:
        """Whether any process of the group is left. Reap the leader first, or its zombie counts."""
        try:
            os.killpg(self.pgid, 0)
        except ProcessLookupError:
            return False
        except PermissionError:
            return True
        return True

    def _signal_group(self, sig: int) -> None:
        try:
            os.killpg(self.pgid, sig)
        except (ProcessLookupError, PermissionError):
            pass

    def quit(self, nvim: "Nvim", check_id: str = "Q") -> None:
        # The result is ignored: Neovim may die before it answers.
        nvim.ex_nowait("WolfQuitApplication")
        try:
            code = self.proc.wait(QUIT_TIMEOUT_S)
        except subprocess.TimeoutExpired:
            self.stop()
            raise SmokeFailure(
                check_id,
                f"run {self.n} was still alive {QUIT_TIMEOUT_S}s after :WolfQuitApplication and "
                "was killed. Either CommandDispatcher's .quitApplication never called "
                "NSApp.terminate (the command did not drain, or its case does nothing), or "
                "applicationWillTerminate hung in snapshotNow, wolf_ffi_state_flush or a core "
                "shutdown.",
            ) from None
        if code != 0:
            how = f"was killed by signal {-code}" if code < 0 else f"exited with status {code}"
            raise SmokeFailure(
                check_id,
                f"run {self.n} {how} within {QUIT_TIMEOUT_S}s of :WolfQuitApplication; want "
                "status 0. A signal means a crash during teardown; a non-zero status means an "
                "error exit. Either way state.json may be unflushed.",
            )

    def stop(self) -> None:
        """TERM, a grace period, then KILL on the whole group; a no-op once all of it is gone."""
        if self.alive() or self.group_alive():
            self._signal_group(signal.SIGTERM)
            try:
                self.proc.wait(TERM_GRACE_S)
            except subprocess.TimeoutExpired:
                pass
            deadline = time.monotonic() + TERM_GRACE_S
            while self.group_alive() and time.monotonic() < deadline:
                time.sleep(POLL_INTERVAL_S)
            if self.alive() or self.group_alive():
                self._signal_group(signal.SIGKILL)
                try:
                    self.proc.wait(TERM_GRACE_S)
                except subprocess.TimeoutExpired:
                    log(f"WARN: run {self.n} (pid {self.proc.pid}) survived SIGKILL's grace")
        self.close_log()

    def drain_group(self) -> None:
        """Waits for the exited app's group to empty, then kills and logs whatever is left."""
        deadline = time.monotonic() + TERM_GRACE_S
        while self.group_alive() and time.monotonic() < deadline:
            time.sleep(POLL_INTERVAL_S)
        if self.group_alive():
            log(
                f"WARN: run {self.n}'s process group {self.pgid} outlived the app by "
                f"{TERM_GRACE_S}s: {group_members(self.pgid)}; killing it"
            )
            self._signal_group(signal.SIGKILL)
        self.close_log()

    def close_log(self) -> None:
        if not self._log_file.closed:
            self._log_file.close()

    def log_tail(self, lines: int = LOG_TAIL_LINES) -> str:
        try:
            with open(self.log_path, "r", encoding="utf-8", errors="replace") as f:
                return "".join(f.readlines()[-lines:])
        except OSError as e:
            return f"(could not read {self.log_path}: {e})"


def group_members(pgid: int) -> str:
    try:
        out = subprocess.run(
            [PGREP, "-l", "-g", str(pgid)],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=RPC_TIMEOUT_S,
            check=False,
        ).stdout.decode("utf-8", "replace")
    except (OSError, subprocess.TimeoutExpired):
        return "(pgrep failed)"
    return " ".join(out.split()) or "(none listed)"


# Each call is one synchronous `--remote-expr` request with its own timeout.
# See notes/tools/wolf-macos-app-smoke.py.md#driving-neovim
class Nvim:
    """The running app's Neovim, reached through the prelude's socket."""

    def __init__(self, client: str, sock: str, env: Dict[str, str], app: Optional[App] = None):
        self.client = client
        self.sock = sock
        self.env = env
        self.app = app
        self.last_probe: Optional[Probe] = None

    def _eval(self, expr: str, timeout: float = RPC_TIMEOUT_S) -> Optional[Tuple[str, str]]:
        """(stdout, stderr) of one evaluation, or None when the client timed out or never ran."""
        try:
            result = subprocess.run(
                [self.client] + list(CLIENT_FLAGS) + ["--server", self.sock, "--remote-expr", expr],
                env=self.env,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=timeout,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired):
            return None
        return (
            result.stdout.decode("utf-8", "replace").strip(),
            result.stderr.decode("utf-8", "replace").strip(),
        )

    def require_app_alive(self, check_id: str) -> None:
        if self.app is None:
            return
        code = self.app.proc.poll()
        if code is not None:
            raise SmokeFailure(
                check_id,
                f"run {self.app.n}'s app exited with status {code} during this step, before it "
                "was asked to quit: a crash, or an exit on a failed launch. See its log tail.",
                self.last_probe,
            )

    def probe(self) -> Optional[Probe]:
        out = self._eval(PROBE_EXPR)
        if out is None:
            return None
        parsed = Probe.parse(out[0])
        if parsed is not None:
            self.last_probe = parsed
        return parsed

    def _run(self, expr: str, what: str, check_id: str) -> None:
        self.require_app_alive(check_id)
        out = self._eval(expr)
        if out is None:
            raise SmokeFailure(
                check_id,
                f"Neovim did not answer {what} within {RPC_TIMEOUT_S}s: it is wedged, or the "
                f"socket {self.sock} is gone.",
                self.last_probe,
            )
        stdout, stderr = out
        if stdout != EX_OK:
            raise SmokeFailure(
                check_id,
                f"{what} did not succeed in Neovim: it returned {stdout!r} (client stderr "
                f"{stderr!r}). E492 means no such command: core never ran its command setup "
                "(wolf_setup_commands), or the init file failed before it; an empty result with "
                "a connection error on stderr means the socket stopped answering.",
                self.last_probe,
            )

    def ex(self, cmd: str, check_id: str) -> None:
        self._run(EX_EXPR.format(arg=vim_string(cmd)), f"`:{cmd}`", check_id)

    def ex_nowait(self, cmd: str) -> None:
        self._eval(EX_EXPR.format(arg=vim_string(cmd)))

    def edit(self, path: str, check_id: str) -> None:
        self._run(EDIT_EXPR.format(arg=vim_string(path)), f"`:edit {path}`", check_id)

    def messages(self) -> Optional[str]:
        out = self._eval(MESSAGES_EXPR, timeout=MESSAGES_TIMEOUT_S)
        return None if out is None else out[0]


def wait_for(
    nvim: Nvim,
    check_id: str,
    describe: Callable[[Optional[Probe]], str],
    predicate: Callable[[Probe], bool],
    timeout: float,
) -> Probe:
    """The first probe ending `STABLE_READS` in a row that satisfy `predicate`.

    On timeout raises `SmokeFailure(check_id)` with `describe(last probe)`, the last probe being
    None when the socket never answered with the probe's JSON.
    """
    deadline = time.monotonic() + timeout
    streak = 0
    last: Optional[Probe] = None
    while True:
        nvim.require_app_alive(check_id)
        p = nvim.probe()
        if p is not None:
            last = p
        if p is not None and predicate(p):
            streak += 1
            if streak >= STABLE_READS:
                return p
        else:
            streak = 0
        if time.monotonic() >= deadline:
            raise SmokeFailure(check_id, f"after {timeout}s: {describe(last)}", last)
        time.sleep(POLL_INTERVAL_S)


def stable_size(nvim: Nvim, check_id: str, win_id: int, timeout: float) -> Tuple[int, int]:
    """Window `win_id`'s `(w, h)` once it reads the same on `STABLE_READS` probes in a row."""
    deadline = time.monotonic() + timeout
    history: List[Tuple[int, int]] = []
    while True:
        nvim.require_app_alive(check_id)
        p = nvim.probe()
        w = p.win(win_id) if p is not None else None
        if w is None:
            history = []
        else:
            size = (w.w, w.h)
            if history and history[-1] != size:
                history = []
            history.append(size)
            if len(history) >= STABLE_READS:
                return size
        if time.monotonic() >= deadline:
            raise SmokeFailure(
                check_id,
                f"window {win_id}'s size never held for {STABLE_READS} reads in {timeout}s "
                f"(last reads {history}); the front-end kept resizing the launch pane, or the "
                "window disappeared.",
                nvim.last_probe,
            )
        time.sleep(POLL_INTERVAL_S)


def _leaf_summary(node: object) -> Tuple[Optional[str], object]:
    """(path tail, zoomed field) of a leaf node, or (None, the node) for anything else."""
    if isinstance(node, dict) and node.get("type") == "leaf" and isinstance(node.get("path"), str):
        return (os.path.basename(node["path"]), node.get("zoomed", False))
    return (None, node)


def check_saved_zoom(state_path: str, project: str) -> None:
    """R1: the saved tree is `vertical[za.md unzoomed, zb.md zoomed]` in one tab of one window."""
    try:
        with open(state_path, "r", encoding="utf-8") as f:
            text = f.read()
    except OSError as e:
        raise SmokeFailure(
            "R1",
            f"could not read run 1's state file {state_path} ({e}): applicationWillTerminate's "
            "snapshotNow or wolf_ffi_state_flush wrote nothing, or WOLF_STATE_DIR was ignored.",
        ) from None
    try:
        data = json.loads(text)
    except ValueError as e:
        raise SmokeFailure("R1", f"{state_path} is not JSON ({e}): a torn write.") from None
    where = f"File: {state_path}"
    if not isinstance(data, dict) or data.get("version") != 1:
        version = data.get("version") if isinstance(data, dict) else None
        raise SmokeFailure("R1", f"run 1 saved version {version!r}; want 1. {where}")
    windows = data.get("windows")
    window = windows[0] if isinstance(windows, list) and len(windows) == 1 else None
    tabs = window.get("tabs") if isinstance(window, dict) else None
    if not isinstance(tabs, list) or len(tabs) != 1 or not isinstance(tabs[0], dict):
        count = len(windows) if isinstance(windows, list) else windows
        raise SmokeFailure(
            "R1",
            f"run 1 saved {count!r} window(s) with tabs {tabs!r}; want one window with one tab. "
            "A second window or tab means the launch opened more than the seeded one; a window "
            f"or tab that is not an object means a malformed save. {where}",
        )
    tab = tabs[0]
    if tab.get("cwd") != project:
        raise SmokeFailure(
            "R1",
            f"run 1 saved the tab cwd {tab.get('cwd')!r}; want {project!r}, the seeded project. "
            f"Another directory means Neovim did not start in the restored cwd. {where}",
        )
    layout = tab.get("layout")
    children = layout.get("children") if isinstance(layout, dict) else None
    if (
        not isinstance(layout, dict)
        or layout.get("type") != "split"
        or layout.get("orientation") != "vertical"
        or not isinstance(children, list)
    ):
        raise SmokeFailure(
            "R1",
            f"run 1 saved the layout {layout!r}; want a vertical split. A lone leaf or no layout "
            f"means the split :WolfSplitVertical made was never saved. {where}",
        )
    leaves = [_leaf_summary(c) for c in children]
    want = [(LEFT_FILE, False), (ZOOMED_FILE, True)]
    if leaves != want:
        raise SmokeFailure(
            "R1",
            f"run 1 saved the leaves (tail, zoomed) {leaves!r}; want {want!r}. Both unzoomed "
            "means the save wrote no mark: liveTab() did not pass zoomedPaneId to "
            "SessionSeam.gatherTab. Both zoomed means every leaf of a zoomed tab is marked; only "
            "za zoomed means the mark went to the wrong pane; a third child or a non-leaf "
            f"means the split itself is wrong. {where}",
        )


def _sizes(p: Probe) -> str:
    return ", ".join(f"{w.id}:{w.name or '<unnamed>'} {w.w}x{w.h}" for w in p.wins)


def run_one(run: RunDir, bundle: Bundle, apps: List[App]) -> Tuple[int, int]:
    """Run 1: P0, C3, P1, C1 and Q. Returns L1."""
    step("run 1: launching the seeded restore")
    app = App.launch(run, bundle, 1)
    apps.append(app)
    nvim = Nvim(bundle.nvim_client, app.sock, client_env(run), app)
    try:
        return _run_one_checks(run, app, nvim)
    except SmokeFailure as f:
        f.app = f.app or app
        f.nvim = f.nvim or nvim
        raise


def _run_one_checks(run: RunDir, app: App, nvim: Nvim) -> Tuple[int, int]:
    step("P0: waiting for the socket and the restored za.md")

    def ready(p: Probe) -> bool:
        eds = p.editors()
        return p.cmd == 2 and len(eds) == 1 and eds[0].name == LEFT_FILE

    def describe_p0(p: Optional[Probe]) -> str:
        if p is None:
            return (
                f"the socket {nvim.sock} never answered the probe. Either Neovim never ran the "
                "prelude (WOLF_INIT_LUA not honoured, or Neovim never started), serverstart "
                "failed, or `--remote-expr` prints no value under `--headless`."
            )
        if p.cmd != 2:
            return (
                f"exists(':WolfZoomPane') is {p.cmd}, want 2: core never ran its command setup "
                "(wolf_setup_commands), or the init file failed (the prelude, or the bundled "
                "init.lua it dofile()s); see :messages below."
            )
        eds = p.editors()
        if len(eds) == 1 and not eds[0].name:
            return (
                f"want one editor window showing {LEFT_FILE}, got [{_sizes(p)}]. A lone unnamed "
                "window means, most likely first: the window never became key (screen locked, "
                "or activation refused): a restored view becomes current only when its window "
                "becomes key. Otherwise the seeded state.json was not restored (WOLF_STATE_DIR "
                "ignored, or the file rejected), or the layout rebuild never ran."
            )
        return (
            f"want one editor window showing {LEFT_FILE}, got [{_sizes(p)}]: the launch opened "
            "a different layout than the seeded lone za.md leaf."
        )

    p = wait_for(nvim, "P0", describe_p0, ready, READY_TIMEOUT_S)
    w_za = p.editors()[0].id
    l1 = stable_size(nvim, "P0", w_za, STEP_TIMEOUT_S)
    if not (l1[0] > 1 and l1[1] > 1):
        raise SmokeFailure(
            "P0", f"the launch pane is stable at {l1}; want a sized grid (w > 1, h > 1).", p
        )
    passed("P0", f"za.md restored in window {w_za}, L1 = {l1[0]}x{l1[1]}", nvim.last_probe)

    step("C3: :WolfSplitVertical")
    nvim.ex("WolfSplitVertical", "C3")

    def split(p: Probe) -> bool:
        eds = p.editors()
        ids = [w.id for w in eds]
        return (
            len(eds) == 2
            and w_za in ids
            and p.cur in ids
            and p.cur != w_za
            and all(w.w < l1[0] and w.h == l1[1] for w in eds)
        )

    def describe_c3(p: Optional[Probe]) -> str:
        if p is None:
            return "the socket stopped answering the probe."
        eds = p.editors()
        head = f"editor windows [{_sizes(p)}], current {p.cur}; "
        if len(eds) == 1:
            return head + (
                "one window: :WolfSplitVertical never reached CommandDispatcher's "
                ".splitVertical, or splitPane(.vertical) split nothing."
            )
        if len(eds) != 2:
            return head + f"want exactly two, got {len(eds)}."
        if p.cur == w_za or p.cur not in [w.id for w in eds]:
            return head + f"want the current window to be the new one, not {w_za}."
        if w_za not in [w.id for w in eds]:
            return head + f"the launch window {w_za} is gone, so the split replaced it."
        return head + (
            f"want both narrower than L1.w {l1[0]} and as tall as L1.h {l1[1]}: the front-end "
            "did not lay the two panes out side by side in the launch pane's area."
        )

    p = wait_for(nvim, "C3", describe_c3, split, STEP_TIMEOUT_S)
    passed("C3", f"split into {[w.id for w in p.editors()]}, new current {p.cur}", p)

    step(f"P1: :edit {ZOOMED_FILE} in the new window")
    nvim.edit(os.path.join(run.project, ZOOMED_FILE), "P1")

    def edited(p: Probe) -> bool:
        cur = p.current()
        za = p.win(w_za)
        return (
            cur is not None
            and cur.id != w_za
            and cur.name == ZOOMED_FILE
            and za is not None
            and za.name == LEFT_FILE
            and len(p.editors()) == 2
        )

    def describe_p1(p: Optional[Probe]) -> str:
        if p is None:
            return "the socket stopped answering the probe."
        return (
            f"want the current window showing {ZOOMED_FILE} and {w_za} still showing "
            f"{LEFT_FILE}; got [{_sizes(p)}], current {p.cur}. The :edit landed in another "
            "window, or something moved Neovim's current window off the new pane."
        )

    p = wait_for(nvim, "P1", describe_p1, edited, STEP_TIMEOUT_S)
    w_zb = p.cur
    passed("P1", f"W_za = {w_za}, W_zb = {w_zb}", p)

    step("C1: :WolfZoomPane")
    nvim.ex("WolfZoomPane", "C1")

    def zoomed(p: Probe) -> bool:
        zb = p.win(w_zb)
        za = p.win(w_za)
        return (
            p.cur == w_zb
            and zb is not None
            and (zb.w, zb.h) == l1
            and za is not None
            and za.w < l1[0]
        )

    def describe_c1(p: Optional[Probe]) -> str:
        if p is None:
            return "the socket stopped answering the probe."
        zb = p.win(w_zb)
        za = p.win(w_za)
        head = f"windows [{_sizes(p)}], current {p.cur}, L1 {l1[0]}x{l1[1]}; "
        if zb is None:
            return head + f"zb's window {w_zb} is gone."
        if (zb.w, zb.h) != l1:
            return head + (
                f"zb is {zb.w}x{zb.h}, want L1. About half L1.w means :WolfZoomPane zoomed "
                "nothing: CommandDispatcher's .zoomPane did not call toggleZoom, or the zoom did "
                "not resize zb's grid to the whole pane area."
            )
        if p.cur != w_zb:
            return head + f"Neovim's current window is {p.cur}, want zb's {w_zb}."
        if za is None:
            return head + f"za's window {w_za} is gone: the zoom closed it rather than hiding it."
        return head + (
            f"za is {za.w} wide, want under L1.w: the hidden pane was resized to the whole area."
        )

    p = wait_for(nvim, "C1", describe_c1, zoomed, STEP_TIMEOUT_S)
    passed("C1", f"zb zoomed to {l1[0]}x{l1[1]}, za hidden at its size", p)

    step("Q: :WolfQuitApplication (run 1)")
    app.quit(nvim)
    passed("Q", "run 1 exited with status 0")
    app.drain_group()
    return l1


def run_two(run: RunDir, bundle: Bundle, apps: List[App], l1: Tuple[int, int]) -> None:
    """Run 2: P2, R2, C2, C2z and Q."""
    step("run 2: relaunching on run 1's state")
    app = App.launch(run, bundle, 2)
    apps.append(app)
    nvim = Nvim(bundle.nvim_client, app.sock, client_env(run), app)
    try:
        _run_two_checks(app, nvim, l1)
    except SmokeFailure as f:
        f.app = f.app or app
        f.nvim = f.nvim or nvim
        raise


def _run_two_checks(app: App, nvim: Nvim, l1: Tuple[int, int]) -> None:
    step("P2: waiting for the socket and the restored split")

    def ready(p: Probe) -> bool:
        return p.cmd == 2 and len(p.editors()) == 2

    def describe_p2(p: Optional[Probe]) -> str:
        if p is None:
            return f"the socket {nvim.sock} never answered the probe in run 2."
        if p.cmd != 2:
            return (
                f"exists(':WolfZoomPane') is {p.cmd}, want 2 in run 2: core never ran its "
                "command setup (wolf_setup_commands), or the init file failed."
            )
        eds = p.editors()
        if len(eds) == 1 and not eds[0].name:
            return (
                f"want two editor windows, got [{_sizes(p)}]. A lone unnamed window means, most "
                "likely first: run 2's window never became key (screen locked, or activation "
                "refused while another app was in use), so no view became current and the saved "
                "split was never rebuilt. Otherwise the restore read no layout."
            )
        return (
            f"want two editor windows, got [{_sizes(p)}]: the saved split was not rebuilt (the "
            "restore read no layout, or the rebuild fell back to a single pane)."
        )

    wait_for(nvim, "P2", describe_p2, ready, READY_TIMEOUT_S)
    passed("P2", "run 2 is up with two editor windows", nvim.last_probe)

    step("R2: the relaunch is zoomed on zb.md")

    def restored(p: Probe) -> bool:
        eds = p.editors()
        cur = p.current()
        others = [w for w in eds if w.id != p.cur]
        return (
            len(eds) == 2
            and not p.drawers()
            and cur is not None
            and cur.name == ZOOMED_FILE
            and (cur.w, cur.h) == l1
            and len(others) == 1
            and others[0].w < l1[0]
        )

    def describe_r2(p: Optional[Probe]) -> str:
        if p is None:
            return "the socket stopped answering the probe."
        cur = p.current()
        head = f"windows [{_sizes(p)}], current {p.cur}, L1 {l1[0]}x{l1[1]}; "
        if len(p.editors()) != 2:
            return head + "want two editor windows: the rebuilt split lost a pane."
        if p.drawers():
            return head + "a drawer is open, but run 1 never opened one."
        if cur is None or cur.name != ZOOMED_FILE:
            return head + (
                f"want {ZOOMED_FILE} current: {LEFT_FILE} current means the rebuild left Neovim "
                "on the first leaf rather than the zoomed one."
            )
        if (cur.w, cur.h) != l1:
            return head + (
                f"zb is {cur.w}x{cur.h}, want L1. About half L1.w means the restore rebuilt the "
                "split but did not re-zoom it (rebuildLayout's zoomedLeafIndex block)."
            )
        return head + "the other window is not narrower than L1.w: it was not hidden at its size."

    p = wait_for(nvim, "R2", describe_r2, restored, STEP_TIMEOUT_S)
    w_zb = p.cur
    passed("R2", f"run 2 zoomed on {ZOOMED_FILE} (window {w_zb}) at L1", p)

    step("C2: :WolfTerminal from the restored zoom")
    nvim.ex("WolfTerminal", "C2")

    def drawer_open(p: Probe) -> bool:
        d = p.drawer()
        return d is not None and p.cur == d.id and 0 < d.h < l1[1]

    def describe_c2(p: Optional[Probe]) -> str:
        if p is None:
            return "the socket stopped answering the probe."
        head = f"windows [{_sizes(p)}], drawers {[w.id for w in p.drawers()]}, current {p.cur}; "
        if not p.drawers():
            return head + (
                "no window carries w:wolf_drawer: :WolfTerminal never reached "
                "CommandDispatcher's .terminal, or toggleTerminal opened nothing."
            )
        if len(p.drawers()) != 1:
            return head + "want exactly one drawer window."
        d = p.drawers()[0]
        if p.cur != d.id:
            return head + "want the drawer current: the open left Neovim in another window."
        return head + f"the drawer is {d.h} tall, want 0 < h < L1.h {l1[1]}."

    p = wait_for(nvim, "C2", describe_c2, drawer_open, STEP_TIMEOUT_S)
    passed("C2", f"drawer {p.cur} open and current", p)

    step("C2z: Toggle Terminal unzoomed zb")

    def unzoomed(p: Probe) -> bool:
        zb = p.win(w_zb)
        return zb is not None and zb.w < l1[0] and zb.h < l1[1]

    def describe_c2z(p: Optional[Probe]) -> str:
        if p is None:
            return "the socket stopped answering the probe."
        zb = p.win(w_zb)
        head = f"windows [{_sizes(p)}], L1 {l1[0]}x{l1[1]}; "
        if zb is None:
            return head + f"zb's window {w_zb} is gone."
        if zb.w >= l1[0] and zb.h >= l1[1]:
            return head + (
                f"zb is still {zb.w}x{zb.h}: Toggle Terminal left the restored zoom in place "
                "(no PaneZoom event it set off unzoomed)."
            )
        if zb.w >= l1[0]:
            return head + (
                f"zb is {zb.w} wide, want under L1.w: it is above the drawer but not beside za, "
                "so the unzoom did not restore the split."
            )
        return head + (
            f"zb is {zb.h} tall, want under L1.h: it is beside za but the drawer took none of "
            "its height."
        )

    p = wait_for(nvim, "C2z", describe_c2z, unzoomed, STEP_TIMEOUT_S)
    zb = p.win(w_zb)
    size = f"{zb.w}x{zb.h}" if zb is not None else "?"
    passed("C2z", f"zb unzoomed to {size}", p)

    step("Q: :WolfQuitApplication (run 2)")
    app.quit(nvim)
    passed("Q", "run 2 exited with status 0")
    app.drain_group()


def scenario(run: RunDir, bundle: Bundle, apps: List[App]) -> None:
    l1 = run_one(run, bundle, apps)

    step("R1: run 1's state.json")
    src = os.path.join(run.state, "state.json")
    copy = os.path.join(run.logs, "run1-state.json")
    try:
        shutil.copyfile(src, copy)
    except OSError as e:
        raise SmokeFailure(
            "R1",
            f"run 1 left no state file at {src} ({e}): applicationWillTerminate's snapshotNow "
            "or wolf_ffi_state_flush wrote nothing.",
        ) from None
    check_saved_zoom(copy, run.project)
    passed("R1", f"saved vertical[{LEFT_FILE}, {ZOOMED_FILE} zoomed] (copy at {copy})")

    run_two(run, bundle, apps, l1)


def report_failure(f: SmokeFailure) -> None:
    log(f"FAIL {label(f.check_id)}: {f.message}")
    probe = f.probe or (f.nvim.last_probe if f.nvim is not None else None)
    log(f"    last probe: {probe.to_json() if probe is not None else '(none)'}")
    if f.nvim is not None and f.app is not None and f.app.alive():
        messages = f.nvim.messages()
        log("    :messages:")
        log(messages or ("    (empty)" if messages == "" else "    (no answer)"))
    if f.app is not None:
        log(f"    last {LOG_TAIL_LINES} lines of {f.app.log_path}:")
        log(f.app.log_tail().rstrip("\n") or "    (empty)")


def _rmtree(path: str) -> None:
    try:
        shutil.rmtree(path)
    except FileNotFoundError:
        pass
    except OSError as e:
        log(f"WARN: could not delete {path}: {e}")


def ere_escape(text: str) -> str:
    """`text` as a literal POSIX ERE, for `pgrep -f`; `re.escape` also escapes `-`, which ERE
    leaves undefined."""
    return re.sub(r"([.\[\]\\(){}*+?^$|])", r"\\\1", text)


def cleanup(run: RunDir, apps: List[App], ok: bool, saved: Optional[SavedState]) -> None:
    global _transcript_path
    for app in apps:
        try:
            app.stop()
        except Exception as e:
            log(f"WARN: stopping run {app.n} failed: {e}")
    if saved is not None:
        clean_saved_state(saved)
    # Only argv under the app copy: a user's `tail -f` on a printed log path must survive.
    try:
        pattern = ere_escape(run.app + "/")
        out = subprocess.run(
            [PGREP, "-f", pattern],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=RPC_TIMEOUT_S,
            check=False,
        ).stdout.decode("utf-8", "replace")
        pids = [int(t) for t in out.split() if t.isdigit() and int(t) != os.getpid()]
        if pids:
            log(f"WARN: killing leftover processes under {run.app}: {pids}")
            for pid in pids:
                try:
                    os.kill(pid, signal.SIGKILL)
                except (ProcessLookupError, PermissionError):
                    pass
    except (OSError, subprocess.TimeoutExpired) as e:
        log(f"WARN: the leftover-process sweep failed: {e}")
    _rmtree(run.app)
    _rmtree(run.sock)
    if ok:
        _transcript_path = None
        _rmtree(run.root)
        return
    _rmtree(run.init)
    _rmtree(run.tmp)
    log(f"run directory kept at {run.root} (logs/, state/, config/, project/, client/)")


def _raise_interrupted(signum: int, _frame: object) -> None:
    raise Interrupted(signum)


HANDLED_SIGNALS = (signal.SIGINT, signal.SIGTERM, signal.SIGHUP)


def main(argv: List[str]) -> int:
    parser = argparse.ArgumentParser(
        description="Launch the built Wolf.app twice and check a zoom survives quit and relaunch."
    )
    parser.add_argument("--app-zip", required=True, help="the Wolf.zip bazel built")
    args = parser.parse_args(argv)

    for sig in HANDLED_SIGNALS:
        signal.signal(sig, _raise_interrupted)

    run: Optional[RunDir] = None
    apps: List[App] = []
    saved: Optional[SavedState] = None
    ok = False
    code = 2
    try:
        try:
            refuse_unless_darwin_outside_regina()
            zip_path = os.path.abspath(args.app_zip)
            if not os.path.isfile(zip_path):
                raise SetupError(f"--app-zip {zip_path} is not a file")
            stamp = time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(os.path.getmtime(zip_path)))
            log(f"app zip: {zip_path} (modified {stamp})")
            run = make_run_dir()
            log(f"run directory: {run.root}")
            bundle = extract_bundle(zip_path, run)
            write_fixtures(run)
            saved = record_saved_state()
            scenario(run, bundle, apps)
            ok = True
            code = 0
            log("PASS: all checks green")
        except SmokeFailure as f:
            code = 1
            report_failure(f)
        except SetupError as e:
            code = 2
            log(f"SETUP ERROR: {e}")
        except Exception:
            code = 2
            log("HARNESS ERROR:")
            log(traceback.format_exc().rstrip("\n"))
    except Interrupted as i:
        code = 128 + i.signum
        log(f"INTERRUPTED by signal {i.signum}")
    finally:
        # A second signal must not cut the cleanup short; every wait in it is bounded.
        for sig in HANDLED_SIGNALS:
            signal.signal(sig, signal.SIG_IGN)
        if run is not None:
            cleanup(run, apps, ok, saved)
    return code


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
