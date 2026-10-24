# Plan: run the wolf-gnome cage smokes in parallel (step 2)

## Status

DONE (step 2, uncommitted).

The final gate ran at the default job count, with no manual `XDG_RUNTIME_DIR`. fmt, prettier, clippy, build and test all exit 0. The workspace has 1178 passed, 0 failed, 10 ignored. smoke_headless reports `73 passed … 60.32s`. `time cargo test --workspace` is real 2m39.6s.

markdown_check findings are only the ones that predate this step: big-file, and the `pane_view.rs`/`tabs_ui.rs` budgets. Process snapshots show no diff.

STEP 9 (technical writer, then coder: prettier, markdown_check, full gate) — complete. The fontconfig finding is recorded in tests/README "What is still unproven".

- `CageSlot` and exclusive mode are deleted, and `acquire(limit)` waits on `running >= limit`.
- D-1, D-4, D-5 (mean-of-two rule gives 16: 64.7 s against 73.25 s at 12, bound 71.2 s), D-7 and D-8 are fixed.
- `flake.nix` counts are 60/73/13.
- fmt and clippy are clean, and the guards show 13 passed.
- G13 (b) and (a) falsifiers went red and were restored (sha c98213…50dc).

STEP 8c. M1/M2 measured by a fresh coder (logs in `scratchpad/step2b/`):

- **M1:**
  - **A:** with only the `XDG_RUNTIME_DIR` env line deleted, it stays green with an `org.wolf.Wolf` owner live on `$S/bus`; nothing reached that bus.
  - **B:** with `dbus-run-session` removed as well, it goes red with exit 11, and a full hand-off shows on the wire (RequestName returns 3 EXISTS, then Activate to the owner, which opens window/2).
  - **C:** restored, green.
  - So `dbus-run-session` is what blocks the hand-off; the private dir gives Wayland/WebKit/runtime-file separation plus defence in depth.
- **M2 (N=16, dirs kept):**
  - No mesa disk-cache or shader warnings; the only mesa hits are wlroots' renderer banners.
  - Fontconfig: `Cannot load default config file: File not found: $TMPDIR/wolf-fonts.*.conf` appears 9× in exactly the 7 WebKit cages. WebKit's sandboxed web process can't see the nix-shell TMPDIR, so the preview falls back to the default fonts.
  - The fontconfig issue is not caused by parallelism (it is a pre-existing sandbox path issue) and is out of scope. It is reported to the user and recorded by the technical writer under tests/README "What is still unproven", if it fits there.

The decisions in STEP 8b stand. Next: the test-writer, then the coder, then the technical writer, then the coder's final gate.

STEP 8b. Reviewer: approved, no correctness defects. Decisions on its findings:

- **D-2: delete `CageSlot::Exclusive`, per §4.** No smoke uses it, and no triage needed it. That means deleting the enum, its arms, the `exclusive`/`exclusive_waiting` fields and the writer-preference doc. The test-writer removes G13 part (c) and renames G13, for example to `cage_slots_bound_concurrency`, along with its module-doc bullet.
- **D-1: measure first.** A fresh coder re-runs the e2e falsifier under R2 (`XDG_RUNTIME_DIR` env deleted, `dbus-run-session` kept, an owner on `$S/bus`). If it is green, the wording becomes: `dbus-run-session` is what blocks the hand-off, and the private dir is Wayland/WebKit/runtime-file separation plus defence in depth. G12 check 4's message ("GIO connects to `$XDG_RUNTIME_DIR/bus`") is false inside the cage either way, so it must be fixed.
- **H-mesa/H-fontconfig evidence:** one full run at N=16 with the runtime dirs kept by a temporary edit, then grep the cage.logs.
- **Code-comment and count fixes (coder):**
  - D-4: `require_smoke_env` "five tools" → six; "five paths" → six; the `flake.nix:588-591` counts → 60 / 73 / 13.
  - D-5: name the statistic as mean of the two runs.
  - D-7: reword.
  - D-8: note the explicit ceiling of about 30 next to `parse_smoke_jobs`.
  - The D-1 code docs in the `CageRuntime` doc.
- **Test-writer:**
  - the G12 check 1 and check 4 messages and the G12 "What it closes" section;
  - the module-doc G12 bullet;
  - the needle guard's "eighth loss" doc and message (`:11041`, `:11085`);
  - the check 10 message nit (print only the first five args).
- **Technical writer:** DECISIONS:341 (D-1 wording, and D-3: count the load-bearing details correctly); tests/README D-6; the `WOLF_SMOKE_JOBS` link nit.
- **Coder final:** fmt, clippy, guards, G13(b) falsifier, then the full gate.

STEP 8 (reviewer). The coder finished at about 10:19, but its hand-back reports arrived empty twice, and it did not write `scratchpad/step2/REPORT.md` when asked. The evidence below comes from its logs in `scratchpad/step2/`, read by the orchestrator.

- **R2 is implemented:** `cage -- dbus-run-session -- sh -c`.
  - The four trial smokes passed (`4 passed … 49.47s`).
  - H-isolation was re-run: `1 passed … 3.02s`, and the monitor printed only its header ("does not have an owner").
- **Check 10 falsifier:** red (`fals/g12_c10.log`). All three builders were reported with `["--", "sh", "-c", …]`.
- **Sweep** (`finished in`, every run 73 passed):

  | N | Time |
  | --- | --- |
  | 1 | 543.2 s |
  | 2 | 280.3 s |
  | 4 | 137.4 s |
  | 8 | 77.1 s, 75.1 s |
  | 12 | 76.0 s, 70.5 s |
  | 16 | 65.2 s, 64.2 s |

  The default is 16: `CPUS_PER_CAGE=2`, `CAP=16`. The table is in the `DEFAULT_SMOKE_JOBS_CAP` doc.
- **Five runs at the default:** 66.40, 65.16, 76.60, 71.33 and 64.47 s, all green.
- **Stress at 24:** 60.68 and 59.52 s, green.
- **Triage:** no failures were seen in any run.
- **H-sock-len:** the longest socket path was 74 bytes (`webkitgtk/a11y-proxy-XXXXXX`, 27 bytes past the directory), so `RUNTIME_DIR_MAX_BYTES = 71` (107 − 27 − 1 − 8).
- **H-x11:** up to 17 X sockets and 1 Xwayland process were seen at once, with 0 `dbus-launch`.
- **Package smoke:** `3 passed`, exit 0.
- **Gate:** fmt, prettier, clippy, build and test all exit 0. The workspace has 1178 passed, 0 failed, and smoke_headless reports `73 passed … 84.86s`.
- **Process snapshots:** no diff, no leftover processes.
- **Unknown:** the increment-1 files appeared staged in the git index during this step. Who staged them is unknown. Do not unstage.

## Revision R2: the session bus (orchestrator decision, from coder measurements)

Measured:
- With no neutralizer, every cage child has `DISPLAY=:1` (cage's own Xwayland) and `dbus-launch` on PATH. GIO autolaunches a private `dbus-daemon` per cage, which also spawns Xwayland. It is not the user's bus, and it exits with the cage.
- The dead-socket stub at `<rt>/bus` failed `preview_pane_renders_and_edit_source_returns_under_cage` 4/4. `DBUS_SESSION_BUS_ADDRESS=disabled:` failed it 1/1, and `unset DISPLAY` failed it 1/1. Each failure is "capture B did not contain zzbodyoscar", with the WebView blank. The preview's live body swap needs a reachable session bus.
- `cage -- dbus-run-session -- sh -c …` passed all 4 trial smokes. Its daemon listens on an abstract socket, so `<rt>/bus` stays absent.

Decision:
1. **The cage spawn becomes `cage -- dbus-run-session -- sh -c <prelude+scenario>`.** Each cage gets exactly one private session bus that dies with it, with no autolaunch and no Xwayland start for D-Bus. The prescribed stub and `disabled:` are ruled out.
   - `Command::new("cage")` stays as is, so `CAGE_SPAWN_NEEDLE` is unchanged.
   - Keep `env_remove` of `DBUS_SESSION_BUS_ADDRESS`, `DISPLAY`, `WAYLAND_DISPLAY` and `WAYLAND_SOCKET` in the test process's Command. `dbus-run-session` sets its own address for the child.
2. **Add `pkgs.dbus` to the Linux devShell** so `dbus-run-session` is not taken from the host, and add `dbus-run-session` to the harness's required-tools check.
   - Confirm that the packaged smokes' arms (`PACKAGED_SCRUB_*`) still find it on PATH.
   - If adding the package changes the shellHook exports or the scrub-list guard G1, keep that guard consistent.
3. **Wording.** Rewrite every "no session bus is reachable" or "no bus to reach" phrase to say that each cage has only its own private bus from `dbus-run-session`. That covers the module doc, the `cage_command` doc, `SAVE_AS_GDK_DEBUG`, and DECISIONS/tests README in §8.
   - The two-window smoke's "MANDATORY … no session bus" premise must be made accurate. With a private bus per cage, a second launch inside the same cage while the first instance is alive hands off to it, on that cage's own bus.
   - The coder checks what the two-window smoke actually does, and states the reason that holds.
4. **New G12 check 10 (test-writer).** For `cage_command` and both `packaged_cage_command` arms, `get_args()` must be exactly `--`, `dbus-run-session`, `--`, `sh`, `-c`, followed by the script. Falsify: drop the `dbus-run-session` and `--` args.
5. The G12 checks 4 and 6 stand as written.

## Baseline (from step 3, increment 1, uncommitted, stays)

Every launch in `smoke_headless.rs` is followed by `wait_ready "$app_pid" <code> "<label>"`, defined in `READINESS_PRELUDE`, which `cage_command` prepends.
- `WOLF_READY_FILE` is set per smoke by `smoke_ready_file()`.
- `wait_ready` primes the seat with `wtype ""`, polls for the file, and on timeout kills the app and exits 40.
- The guard `every_launch_waits_for_readiness_before_its_first_input` enforces it.
- The smoke binary reports `71 passed; 3 ignored; finished in ~534s` (five runs: 531.9–536.3 s). Of the 71, 60 are `_under_cage` smokes and 11 are compositor-free guards.
- Under 32 busy-loops on a 32-thread host, `window_renders_typed_text_under_cage` passed 10/10, at 4.9–6.6 s against 2.9 s idle.
- Slowest smokes: `dragged_divider_weights_survive_relaunch_under_cage` at 35–37 s; `two_windows_two_neovims_and_two_saved_sessions_under_cage` at about 12 s; `three_tabs_survive_a_relaunch_under_cage` at about 10.6 s.

## Problem statement

The 60 cage smokes serialize on `static CAGE_LOCK`. Replace that with bounded concurrency so the suite's wall time drops substantially, without adding flakiness.

Known isolation gap: the harness removes `DBUS_SESSION_BUS_ADDRESS`, but GIO falls back to `$XDG_RUNTIME_DIR/bus`. With the user's packaged wolf-gnome running, smoke launches hand off to it (single-instance activation). All measurements so far passed a private `XDG_RUNTIME_DIR` by hand (`/tmp/claude-1000/wolf-xdg`). A private XDG_RUNTIME_DIR per cage (mode 0700) should close the gap and give each cage its own Wayland socket.

### Requirements

- **Concurrency.** Bounded concurrency, such as a counting semaphore.
  - The default job count is chosen by measurement: wall time and flake rate at N = 2, 4, 8, 12, or as measurement suggests.
  - An env var overrides it, and 1 gives today's serial behaviour.
  - Document the knob in tests/README.md, and in build.md if it is a user-facing run knob.
- **Per-cage isolation.** Each cage gets its own copy of everything that must not be shared: XDG_RUNTIME_DIR (Wayland socket, D-Bus fallback, readback files), capture paths, nvim sockets, state and config dirs, and the ready file.
  - Audit every shared path and global in the harness, including the deliberately shared XDG_CACHE_HOME (the shader cache), and justify each.
  - Audit the other binaries in crates/wolf-gnome/tests that launch cage.
- **Guard.** Add a harness-level guard that the smokes cannot reach the user's session bus or Wayland display.
- **No added flakiness.**
  - At least 5 full runs at the chosen N, all green, with per-run wall time.
  - Fix the cause of any flake. A longer sleep is acceptable only with a measured reason. Running a smoke exclusively is an acceptable fallback, with a recorded reason.
- **Packaged smokes.** `./tools/wolf-package-smoke.sh` (the `#[ignore]`d packaged smokes) keeps working.
- **Docs and process.**
  - Docs follow CLAUDE.md: edited in place, present tense, net-negative where possible, prettier afterwards. Covers DECISIONS.md, tests/README.md and build.md.
  - Measurements must not touch the user's running wolf-gnome, and runs must not overlap.
  - The Linux verification gate in build.md passes.
  - Do not commit.

## Orchestrator decisions on the architect plan

- D1: The plan below is accepted as written, including the `flake.nix` change. That change deletes the conditional `XDG_RUNTIME_DIR` mktemp export, because the harness no longer reads the parent's value. Nothing else in the devShell may depend on it; if the coder finds something that does, keep the export and use the plan's fallback rewording of `KEPT_FOR_THE_COMPOSITOR`.
- D2: Order.
  1. The test-writer writes G12 and G13 against the §3 signatures (the contract), plus the count changes, in `smoke_headless.rs`.
  2. The coder lands §9 steps 1–4, compiles both, and runs the guards and their falsifiers.
  3. The coder then runs the measurement protocol (§6), starting with H-dbus-1, H-isolation and the end-to-end falsifier, before any N-sweep.
- D3: If H-dbus-1 forces a neutralizer, the coder picks the dead-socket stub first. It reports to me before changing G12 checks 4 and 6, and I route those to the test-writer.
- D4: Stop-and-report conditions. Any of these stops the coder's work, and it reports to me rather than working around it:
  - H-baseline is off by more than 5 %;
  - H-isolation shows a hand-off;
  - more than three distinct smokes flake at the default N.
- D5: Never kill or signal the user's wolf-gnome (it had pid 78756 earlier; re-snapshot it). Kill only pids captured with `$!`.

# Architect plan: run the wolf-gnome cage smokes in parallel

## 1. Summary

Replace `static CAGE_LOCK` (smoke_headless.rs:463) with a per-process counting semaphore, `CageSlots`. It has a writer-preferring exclusive mode. The job count comes from `WOLF_SMOKE_JOBS`, with a default derived from `available_parallelism` and fixed by measurement.

Isolation comes from a new `CageRuntime`: a slot plus a fresh private 0700 runtime dir, which `cage_command` takes by reference, so the types guarantee no cage spawns without both. `cage_command` also:
- `env_remove`s `WAYLAND_DISPLAY`, `WAYLAND_SOCKET` and `DISPLAY`;
- keeps `DBUS_SESSION_BUS_ADDRESS` removed, adding a neutralizer only if H-dbus-1 requires one;
- writes cage stdout and stderr to `<rt>/cage.log`, so parallel output does not interleave.

Two compositor-free guards are added: the counts go from 11 guards / `71 passed` to 13 / `73 passed`.

## 2. Isolation audit

The test-process `runtime_dir` comes from `require_smoke_env()` (2499), which reads the parent's `XDG_RUNTIME_DIR` at 2539.

| Shared thing | Today | Change |
|---|---|---|
| `XDG_RUNTIME_DIR` (`wayland-N` socket and `.lock`) | Inherited and shared | A fresh `<temp_dir>/wolf-smoke-rt-<pid>-<seq>`, mode 0700, per cage, created in `CageRuntime::fresh`. Removed in `Drop` on success; kept and reported on panic. |
| D-Bus. GIO tries the env var, then `$XDG_RUNTIME_DIR/bus`, then `dbus-launch --autolaunch` if `DISPLAY` is set | The env var is removed (10624), but the fallback reaches `/run/user/1000/bus`, so launches hand off | The private dir has no `bus`. Remove `DISPLAY`. H-dbus-1 decides whether a neutralizer is needed. |
| GApplication single instance | Hands off when a bus is reachable | With no bus, it proceeds as a non-unique application (glib `gapplicationimpl-dbus.c`). The two-window and relaunch smokes launch one after another, so they are unaffected. The baseline already ran this way. |
| AT-SPI, dconf, gvfs, WebKit `$XDG_RUNTIME_DIR/webkitgtk` proxy sockets, pulse | Shared, and reach the user's session | Per cage, via the runtime dir. |
| Captures, readbacks and frozen state dirs (`runtime_dir.join(...)`, ~100 `*_NAME` consts at 1791–37701) | Under the shared dir. Names are distinct, but the files land in `/run/user/1000` | `require_smoke_env()` returns `CageRuntime`, which implements `Deref<Target = Path>`, so call sites are unchanged. |
| `WOLF_READY_FILE` (`smoke_ready_file` 10465, `SMOKE_READY_SEQ` 10457) | Unique under `temp_dir()` | Becomes `<rt>/READY_FILE_NAME`. Delete `smoke_ready_file`; rename the seq to `SMOKE_RUNTIME_SEQ` and use it for the dir name. |
| Captive nvim socket | Inherits the app's runtime dir (`wolf_core::nvim::spawn`, nvim.rs:1788) | Per cage. H-nvim-sock is informational. |
| `sun_path` (108 bytes) | — | Assert the runtime dir path length is at most `RUNTIME_DIR_MAX_BYTES` (provisional value 64, set by H-sock-len). |
| `wolf-pointer-inject` FIFO and ack files | `cwd.join(...)` (11323–11332), per smoke | None. |
| cwd, `HOME`, `WOLF_STATE_DIR`, `WOLF_CONFIG_DIR` | `make_temp_cwd` (9823) uses pid plus nanos with `create_dir_all`, so a same-nanosecond pair would share silently | Add `-{seq}` from a process-wide `AtomicUsize` and use `std::fs::create_dir`, so a collision fails loudly. Update the `TEMP_CWD_PREFIX` doc (9838). |
| `XDG_CACHE_HOME` shader cache (`smoke_shader_cache_dir` 10277) | Shared on purpose | Keep it shared (H-mesa). |
| Fontconfig cache | Shared | Keep it shared (H-fontconfig). |
| wtype, grim and pointer-inject inside cage | Reach cage via `WAYLAND_DISPLAY` under the runtime dir | They inherit the private dir. `WAYLAND_SOCKET` is removed. |
| Host `WAYLAND_DISPLAY` and `DISPLAY` | Inherited | `env_remove` both, plus `WAYLAND_SOCKET`. |
| X11 display numbers (if cage has Xwayland) | Global | wlroots locks them, so this is safe; H-x11 caps stress N at about 24. |
| tesseract | Test process | No collision. H-omp is optional. |
| Statics | `CAGE_LOCK` and `SMOKE_READY_SEQ`; no `OnceLock`, `thread_local` or `LazyLock` | Replace both. |
| `std::env::set_var` | None in this file | None. |

## 3. Files to modify

### `crates/wolf-gnome/tests/smoke_headless.rs`

New items (near the old `CAGE_LOCK`, 449ff, or beside `cage_command`):
- `const SMOKE_JOBS_ENV: &str = "WOLF_SMOKE_JOBS";`
- `const DEFAULT_SMOKE_JOBS_CPUS_PER_CAGE: usize = 4;` and `const DEFAULT_SMOKE_JOBS_CAP: usize = 8;`. Both are provisional; their doc comment receives the measured table.
- `fn parse_smoke_jobs(raw: Option<&str>, parallelism: usize) -> Result<usize, String>`:
  - `None` gives `(parallelism / CPUS_PER_CAGE).clamp(1, CAP)`.
  - `Some(s)` gives the trimmed `usize` if it is at least 1, else `Err` (`""`, `"0"`, `"x"`, `"-1"`).
  - An explicit value above the cap is allowed.
- `fn smoke_jobs() -> usize`: an `OnceLock`-cached parse of the env var and `available_parallelism`. It panics with the `Err` text, naming the variable.
- `#[derive(Clone, Copy, Debug, PartialEq, Eq)] enum CageSlot { Shared, Exclusive }`
- `struct CageSlots { state: Mutex<SlotState>, freed: Condvar }`, with `SlotState { running: usize, exclusive: bool, exclusive_waiting: usize }`:
  - `const fn new()`.
  - `fn acquire(&self, limit: usize, mode: CageSlot) -> CageSlotGuard<'_>`.
  - Shared waits (`Condvar::wait_while`) while `exclusive || exclusive_waiting > 0 || running >= limit`.
  - Exclusive increments `exclusive_waiting`, waits while `exclusive || running > 0`, then sets `exclusive`.
  - Recover from poisoning with `PoisonError::into_inner`.
- `struct CageSlotGuard<'a>`: `Drop` releases the slot and calls `notify_all`.
- `static CAGE_SLOTS: CageSlots = CageSlots::new();`
- The semaphore's doc comments state:
  - it is per process and cannot bound across two `cargo test` invocations;
  - the slot is held for the whole body, including OCR;
  - exclusive mode is writer-preferring;
  - the effective concurrency is `min(WOLF_SMOKE_JOBS, --test-threads)`.
- `const RUNTIME_DIR_PREFIX: &str = "wolf-smoke-rt-";` (it must not start with `TEMP_CWD_PREFIX`).
- `const RUNTIME_DIR_MAX_BYTES: usize = 64;` (provisional).
- `const READY_FILE_NAME: &str = "wolf-ready";`
- `const CAGE_LOG_NAME: &str = "cage.log";`
- `struct CageRuntime<'a> { dir: PathBuf, _slot: CageSlotGuard<'a> }`:
  - `fn fresh(slot: CageSlotGuard<'a>) -> Self` creates `temp_dir()/RUNTIME_DIR_PREFIX<pid>-<seq>` with `DirBuilder::new().mode(0o700).create()` (non-recursive, so it fails if the path exists). It asserts the path length, panicking with a message that names `TMPDIR`.
  - `impl Deref<Target = Path>`.
  - `Drop`: if `std::thread::panicking()`, `eprintln!` the kept path and about the last 80 lines of `cage.log`; otherwise `remove_dir_all`, ignoring errors.
  - Doc decisions:
    - It lives under `temp_dir()`, not beside the cwd, because the typed-`:cd` smoke re-roots to the cwd's parent.
    - Removing it on success bounds the footprint.
    - `cage_command` takes it by reference, which gives the type-level guarantee.
    - It must be bound to a named variable: `let _ =` releases the slot at once.

Changed items:
- `require_smoke_env()` (2499) returns `CageRuntime<'static>`.
  - It acquires `CAGE_SLOTS.acquire(smoke_jobs(), CageSlot::Shared)` first, then checks tools, `WOLF_NVIM_BIN` and `WOLF_RG_BIN`.
  - Delete the parent `XDG_RUNTIME_DIR` check (2539–2559) and its doc sentence.
  - Add `require_smoke_env_exclusive()` only if triage needs it.
  - The 60 `let runtime_dir = require_smoke_env();` call sites are unchanged (`_runtime_dir` at 18632 is fine).
- `cage_command(scenario: &str, cwd: &Path, runtime: &CageRuntime<'_>) -> Command` (10601):
  - `.env("XDG_RUNTIME_DIR", runtime.as_ref())`;
  - `WOLF_READY_FILE` = `runtime.join(READY_FILE_NAME)`;
  - keep `.env_remove("DBUS_SESSION_BUS_ADDRESS")`;
  - `.env_remove` `WAYLAND_DISPLAY`, `WAYLAND_SOCKET` and `DISPLAY`;
  - stdout and stderr go to `File::create(runtime.join(CAGE_LOG_NAME))`, one handle via `try_clone`;
  - keep `Command::new("cage")` contiguous (`CAGE_SPAWN_NEEDLE`);
  - rewrite the "seven variables" doc list in place.
- `run_pointer_scenario` (11315) gains `runtime: &CageRuntime<'_>`. Update its callers at 17380, 18870, 19058, 19357, 19555, 20570, 28130 and 32990.
- `packaged_cage_command` (36187) gains `runtime: &CageRuntime<'_>` before `gtk_runtime`. Update callers 36613, 37390 and 37982, and rewrite the doc's "seven" list.
- Delete `static CAGE_LOCK` and its doc (449–463). Delete every `let _cage_guard = CAGE_LOCK.lock()…` line and its two-line comment (about 60 sites, 15113–37964). Pass `&runtime_dir` to every `cage_command(...)` call (15159–35711, plus 21984 and 28961).
- Reword or cut doc and comment mentions of `CAGE_LOCK`:
  - 157–162: keep the two-window reason, reworded as "no session bus is reachable".
  - 345–348 and 359–360: the counts.
  - 362–371: keep only the single-spawn-guard reason for the packaged smokes.
  - 397–398: replace with the job knob.
  - 419: delete the `XDG_RUNTIME_DIR` requirement.
  - 5660, 9221, 10721, 10900, 11156, 13304, 13406, 13668, 15050, 17121, 20804, 21517, 21794, 22311, 22699, 24015, 25000, 25385, 26752, 27933, 36338 and 36494.
  - 35988–35999: delete the "runs CONCURRENTLY … cannot serialize" sentences.
- 10765, the guard failure message: use `cage_command(scenario, &cwd, &runtime_dir)`, and add the private runtime dir to the list of what a direct spawn loses.
- 33927 and 34143 (`GDK_DEBUG` note): reword, since no bus is reachable now. The export stays as defence in depth.
- `KEPT_FOR_THE_COMPOSITOR` (36135–36142): remove `XDG_RUNTIME_DIR`, which leaves `[&str; 2]`, and fix "exactly the three names" (36089). This depends on the flake.nix step; if that step is skipped, reword its reason instead. Fix the 10310 parenthetical.
- `make_temp_cwd` (9823): sequence plus `create_dir`, per the audit.
- Frame-stats pre-clear (`PREVIEW_FRAME_STATS_FILE_NAME` 16621, `FRAME_STATS_FILE_NAME` 35707): delete it if it only guards against a previous run's file; keep it if it separates two launches.
- Leave the 60 `if out_png.exists() { remove }` blocks alone.

### `flake.nix` (separable; do it before measuring)

Delete the conditional `XDG_RUNTIME_DIR` mktemp export and its comment (lines 710–716); keep the `WLR_*` part. Shrinking `KEPT_FOR_THE_COMPOSITOR` is enough for `every_shellhook_export_is_scrubbed_or_deliberately_kept`. The `doCheck` comment (838–842) stays.

### `tools/wolf-package-smoke.sh` (27–30) and `tools/README.md` (7)

Rewrite the reason for "do not overlap another `cargo test`": each run sizes its cage slots for the whole host, so two runs oversubscribe it and stretch fixed settles. There is no socket contention.

## 4. Test plan (test-writer)

Both guards are compositor-free, take no global slot and spawn nothing. They add module-doc guard bullets (245–343) and change counts:
- module doc: "Eleven" to "Thirteen", "All eleven" to "All thirteen", "Ten of the eleven" to "Twelve of the thirteen", `71 passed` to `73 passed`;
- `tests/README.md` 123: "Eleven" to "Thirteen", `71 passed` to `73 passed`.

**G12 `every_cage_gets_a_private_runtime_dir_and_no_host_session`.** It uses a local `CageSlots` (limit 2) and `make_temp_cwd()`, builds `rt_a` and `rt_b` via `CageRuntime::fresh(local.acquire(2, Shared))`, runs `cage_command("true", &cwd, &rt_a)` and the same for `rt_b`, and reads `get_envs()`, where a removal shows as `(key, None)`. Checks, each with its falsifying edit:
1. `XDG_RUNTIME_DIR == Some(rt_a)` and it differs from the test process's value. Falsify: delete the `.env("XDG_RUNTIME_DIR", …)`.
2. `rt_a != rt_b`. Falsify: drop the seq from the name; `AlreadyExists` makes it red at construction.
3. Mode `& 0o777 == 0o700`, `is_dir`, and the uid equals the cwd's uid. Falsify: change the mode to `0o755`.
4. No `bus` entry, or exactly the dead stub if H-dbus-1 forces one. Falsify: use the parent's runtime dir.
5. The path length is at most `RUNTIME_DIR_MAX_BYTES`. Falsify: set the max to 10.
6. `DBUS_SESSION_BUS_ADDRESS` is `(key, None)`, or the chosen neutral value. Falsify: delete the `env_remove`.
7. `WAYLAND_DISPLAY`, `WAYLAND_SOCKET` and `DISPLAY` are each `(key, None)`. Falsify: delete one.
8. `WOLF_READY_FILE` starts with `rt_a`. Falsify: point it at `temp_dir()`.
9. Both `packaged_cage_command` arms carry `XDG_RUNTIME_DIR == Some(rt)` and the removals. Falsify: add `"XDG_RUNTIME_DIR"` to `PACKAGED_SCRUB_ALWAYS`.

Non-vacuity: `get_envs()` contains at least `HOME`, `WOLF_STATE_DIR` and `WOLF_CONFIG_DIR`. Clean up the cwd parent.

**G13 `cage_slots_bound_concurrency_and_run_exclusive_smokes_alone`.**

(a) `parse_smoke_jobs` table:

| Input | Expected | Falsify by |
|---|---|---|
| `(Some("1"), 32)` | `Ok(1)` | — |
| `(Some(" 12 "), 32)` | `Ok(12)` | `.min(CAP)` on the override |
| `(None, 32)` | the value computed from the constants, with the literal also asserted | ignoring `parallelism` |
| `(None, 1)`, `(None, 3)` | `Ok(1)` | removing the clamp's lower bound |
| `"0"`, `""`, `"x"`, `"-1"` | `Err` | accepting `0` |

(b) For limits 1 and 3:
- Spawn K = 6 threads on a local `CageSlots`.
- Each acquires, increments `current` and records `fetch_max`.
- While holding, it waits until `current == limit` or all K have entered, with a loud 5 s deadline.
- Then it decrements and releases.
- Assert the observed maximum equals `limit` exactly.
- Falsify: change `running >= limit` to `running > limit`.

(c) Exclusive:
- A shared holder blocks an exclusive request until it releases.
- While exclusive is held, later shared acquirers do not enter (`exclusive_active` is false on entry).
- The exclusive holder samples `current == 1`.
- Falsify: drop `running > 0` from the exclusive wait, or `exclusive ||` from the shared predicate.

If no smoke ends up exclusive, delete `CageSlot::Exclusive`, its arm and part (c).

**End-to-end falsifier** (coder, manual, not committed):
1. Start a private `dbus-daemon --session --nofork --address=unix:path=$S/bus` in the scratchpad.
2. Start an `org.wolf.Wolf` owner on it (`target/debug/wolf-gnome` under its own headless cage, with its own 0700 runtime dir and state, config and `HOME` in `$S`).
3. Check that `NameHasOwner` is true.
4. Temporarily delete `.env("XDG_RUNTIME_DIR", …)`, then run `XDG_RUNTIME_DIR=$S cargo test … -- --exact window_renders_typed_text_under_cage` at `WOLF_SMOKE_JOBS=1`. It must go red: the launch hands off and `wait_ready` exits 11.
5. Restore the line and run it again: green.
6. Kill only the `$!` pids.

## 5. Hypotheses (measure before relying on them)

- **H-baseline:** N=1 gives `73 passed`, `finished in` within about 2 % of 534 s.
- **H-dbus-1:** check `env` and `command -v dbus-launch` inside a cage child, and look for new user-owned `dbus-daemon --session` processes after a run.
  - If cage sets `DISPLAY` (Xwayland) and `dbus-launch` exists, add a neutralizer. First choice: a dead socket stub at `<rt>/bus` (bind a `UnixListener`, then drop it). Second choice: `DBUS_SESSION_BUS_ADDRESS=disabled:`.
  - After a neutralizer, re-measure all 60 smokes.
- **H-isolation:**
  - Record the user's wolf-gnome pids and `NameHasOwner org.wolf.Wolf`.
  - Run `gdbus monitor --session --dest org.wolf.Wolf > $scratch/mon.log &` during a single-smoke run and the first full run, without a manual `XDG_RUNTIME_DIR`.
  - Expect only the header, and the same pids afterwards.
  - Never `pkill`, `killall` or signal anything by name.
- **H-sock-len:** `find $rt -type s` on a kept dir; set `RUNTIME_DIR_MAX_BYTES` to 107 minus the longest relative socket path minus about 8, and record the measurement in the doc.
- **H-nvim-sock:** does `nvim --embed` create `$XDG_RUNTIME_DIR/<app>.<pid>.0`? Informational.
- **H-x11:** with Xwayland, `ls /tmp/.X11-unix` during an N=16 run; keep stress N at or below about 24.
- **H-mesa and H-fontconfig:** no new `disk_cache`, mesa or fontconfig warnings in `cage.log`.
- **H-portal** (informational): save-as passes with `GDK_DEBUG` removed. The export stays either way.
- **H-omp** (optional): `OMP_THREAD_LIMIT=1` on the tesseract calls (33308, 33393, 33517) gives identical OCR. Adopt it only if wall time improves by more than 5 %.

## 6. Measurement protocol

Everything runs through `nix develop --command`, with logs in the scratchpad.
- Build first: `cargo test -p wolf-gnome --test smoke_headless --no-run`.
- Before each run, check that no other `cargo test` or `smoke_headless` process is running. Snapshot the user's `cage`, `wolf-gnome`, `nvim` and `dbus-daemon` pids, and diff them afterwards.
- Record `finished in`, `real` and `user`, the pass/fail counts and the names of any failures.

Steps:
1. H-dbus-1, then H-isolation (one smoke), then the end-to-end falsifier.
2. H-baseline: N=1.
3. One full run each at N = 2, 4, 8, 12 and 16.
4. Default: the smallest N within about 10 % of the best wall time, with zero failures. Set the constants so that the rule yields it on 32 threads, and write the table into their doc.
5. Five full runs at the default, all green.
6. Two stress runs at about twice the default (at most 24). Triage every failure; record a pure-overload failure (a readiness timeout at 30 s) rather than fixing it.
7. If any fix lands, repeat step 5.

Flake triage:
1. Reproduce: 10 runs at N=1, then 10 under contention (`--exact <smoke> <N-1 heavy siblings>`, or full-suite loops).
2. Find the race from the kept runtime dir (captures, readbacks, `cage.log`, exit code). Watch list:
   - crash smokes: `liveness_ui::BANNER_VISIBLE_SECS` (4 s; 4064–4071);
   - stall smoke: `STALL_WAIT=2.0` after `STALL_RPC_TIMEOUT_MS=800`;
   - bufpick: `MRU_GAP=1.2`, where load compresses the `lastused` gap; the fix is to wait for the first `:e` readback;
   - drag and divider smokes: `POINTER_STEP`;
   - settles before captures: `SETTLE`, `TREE_SETTLE`, `PREVIEW_SETTLE`, `RELAUNCH_RESTORE_SETTLE`, `DRAWER_SETTLE`, `TABS_BAR_SETTLE`, `PASTE_SETTLE`, `TOGGLE_SETTLE`, `DIALOG_SETTLE`;
   - preview: `PREVIEW_TYPE_DELAY_MS`;
   - frame-stats: low risk.
3. Fix, in order of preference:
   - a readiness or file wait;
   - a longer settle, with the measured distribution at that N written beside the constant;
   - `require_smoke_env_exclusive()`, with the reason at the call and in the tests README.

   Never weaken an assertion.

Finally, run the Linux gate in build.md at the default N. Do not commit.

## 7. Other binaries, the packaged smokes, cleanup

- Cargo runs test binaries one at a time. No other file spawns `cage` (only 10602), and the 13 guards run freely.
- Two concurrent `cargo test` runs no longer collide, but they oversubscribe; the docs keep "do not overlap".
- `./tools/wolf-package-smoke.sh`: its three packaged smokes run concurrently up to N, each in its own runtime. Run it once after the change. It needs a package built from the current tree; report if it can't run.
- `cage_command` has no kill or timeout; the scenarios carry them (`wait_ready` 40 after 30 s). A panic happens after cage exits, so no cage leaks. A hang holds one slot. A watchdog is a follow-up.
- On success the runtime dir is removed. On failure it is kept, and its path and log tail are printed in the test's failure output.

## 8. Docs (edit in place, net-negative, `prettier --write`, `./tools/lint/markdown_check.sh`)

- **DECISIONS.md, "The headless smoke test":**
  - 339: replace the "second test binary … cannot serialize" reason with the single-spawn-guard reason.
  - 341: the first load-bearing detail becomes the private 0700 runtime dir per cage (its own Wayland socket and GIO bus fallback; without it a launch hands off to a running wolf-gnome), plus bounded cage slots, because load stretches fixed settles.
- **tests/README.md:**
  - smoke entry (121–131): counts, plus one gotcha (a failing smoke keeps its runtime dir with captures and `cage.log`; a passing one removes it); drop the frame-time staleness gotcha if its code goes.
  - 168: "The serialization" becomes "The cage slots and private runtime directories".
  - 192: rewrite the hung-smoke paragraph (one slot is held, and libtest's "running for over 60 seconds" names it).
  - 194–196: rewrite the `GDK_DEBUG` premise.
  - Add one short paragraph on triaging failures that appear only under concurrency.
  - 269, 292 and DECISIONS 415 stay unless `disabled:` is adopted.
- **build.md:**
  - 62: replace the `XDG_RUNTIME_DIR` sentence with `WOLF_SMOKE_JOBS=<n>` (`1` runs them serially; for the default, cite the constants).
  - 138: drop "a writable `XDG_RUNTIME_DIR`" and "a session bus".
  - 144: the new overlap reason.
- **tools/README.md 7 and the script comment:** as above.

## 9. Implementation order

1. The semaphore types and constants; they compile alongside `CAGE_LOCK`.
2. `CageRuntime`; `require_smoke_env` returns it.
3. The signature changes and every caller; delete `CAGE_LOCK`, its lock lines and `smoke_ready_file`.
4. The `make_temp_cwd` seq, the flake.nix and `KEPT_FOR_THE_COMPOSITOR` edits, and the code doc rewrites.
5. G12 and G13 (test-writer) and the counts; run and revert each falsifier.
6. fmt and clippy.
7. Measurement (§6).
8. Defaults, triage, Markdown docs, prettier, the gate.

## 10. Split

- **Test-writer:** G12, G13, the module-doc guard bullets and every count change.
- **Coder:** everything else.

The §3 signatures are the contract.

## 11. Dependencies and lint

- std only: `Mutex`, `Condvar`, `OnceLock`, `available_parallelism`, `DirBuilderExt`, `PermissionsExt`, `MetadataExt`, `UnixListener` (stub only) and `Command::get_envs`.
- No `#[allow]`, and no `unsafe`.
- Use `Condvar::wait_while`.
- `run_pointer_scenario` takes at most 7 params.
- `Command::new("cage")` stays contiguous.
