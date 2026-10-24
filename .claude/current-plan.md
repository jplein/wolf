# Plan: readiness waits instead of fixed sleeps in the wolf-gnome cage smokes

## Status

DONE (increment 1, uncommitted).

Round 5:
- `window.rs`: `eprintln!` changed to `write_all`, and the doc comment reworded.
- Markdown: prettier clean. markdown_check shows only findings that predate this increment (big-file, and the `pane_view.rs`/`tabs_ui.rs` budgets); the coordinator entry was trimmed to fit its budget.
- Guards: 11 passed. Checks 6 and 7 were each seen red.
- Gate: all 5 steps exit 0. Workspace 1176 passed, 0 failed; smoke_headless `71 passed … 536.98s`.

Earlier: STEP 9b. Reviewer: approved, no blockers. Decisions:
- Guard hardening accepted. These two checks are added to `every_launch_waits_…`:
  - (6) the first non-blank, non-comment line after each `app_pid=$!` starts with `wait_ready "$app_pid"`;
  - (7) the count of `app_pid=$!` lines equals the count of launch lines.
- Comment fixes in smoke_headless.rs go to the test-writer:
  - the dangling pointer at about :17138 and the missing heading cited at about :10712;
  - the "six"/"four"/tally counts at about :10688, :10509 and :10745;
  - the past-tense phrasing ("replaced", "the old launch probe") at about :294, :10782, :10848, :10854 and :10957.
- The technical writer handles:
  - tests/README :186, "Nothing sleeps after `wait_ready`", which is wrong (TREE_SETTLE follows it at three sites). The guard enforces only no sleep between a launch and its `wait_ready`.
  - Owner overlap: the src/README `ready_seam.rs` Decisions bullets restate DECISIONS.md.
  - The DECISIONS wording "physical keyboard device".
  - The E4 entry in "What is still unproven".
- The coder then runs the guard, falsifiers for checks 6 and 7, clippy and fmt, and the markdown checks.
- Tell the user: `./tools/wolf-package-smoke.sh` exits 40 until the package is rebuilt from a commit containing `ready_seam.rs`.

The technical writer is done:
- tests/README: the after-`wait_ready` wording, the guard's three checks, and the E4 "unproven" entry.
- src/README: the duplicated bullets are cut.
- DECISIONS: virtual keyboard / non-logical devices.

It had no shell, so the coder runs prettier and markdown_check on tests/README.md, DECISIONS.md and src/README.md.

Added for the coder's final round:
- `window.rs` about :945: the new "no frame clock after present()" diagnostic uses `eprintln!`, which breaks the DECISIONS:359 rule. Switch it to `write_all` with the result discarded.
- `window.rs` about :916: the comment says "The PHYSICAL devices". Reword it to "non-logical devices (the `wtype` virtual keyboard counts)".

Previous: STEP 9 (reviewer running → technical writer). Round 4 is green. T12 is now a `StepCase` row struct. `cargo fmt --check` passes with exit 0, and plain `cargo clippy --workspace --all-targets -D warnings` passes with exit 0. `ready_seam` has 15 passed, and the T12 falsifier went red again. The round 3 gate covers build and test, since only test code changed. Coder round 3 done. All 15 unit tests and the guard pass; every unit falsifier (T1–T14, check order) and all 5 guard falsifiers went red, each restored by checksum. E1 red: exit 40 at 32.10 s. E2 red in 5/5, OCR without "hello", signature (a). E4 green in 10/10, and neither relaunch smoke ever logs `layout-pending`/`restore-owed`, because the synchronous wake in `window::build` settles the restore before the first check. That goes into tests/README "What is still unproven", where those two conjuncts' only falsifiers are their unit rows (technical writer). Flake: five full runs, all `71 passed`, at 536.34 / 534.34 / 533.58 / 531.94 / 533.09 s, against a 580.18 / 578.47 s baseline. Relaunch and save smokes passed 5/5 each, and the typed-text smoke passed 10/10 under 32 busy-loops. Gate: fmt, prettier, build and test (1176 passed, 0 failed) are green. Clippy is red: `clippy::type_complexity` at `ready_seam.rs:991`, the `cases` tuple array in `next_step_table`, which goes to the test-writer.

Previous: STEP 8 (coder round 3: run new tests, falsifiers, flake loops, gate). Test-writer done: 15 unit tests in ready_seam.rs (T1–T14 + `blockers_are_reported_in_check_order`), guard `every_launch_waits_for_readiness_before_its_first_input` (5 checks, floor 62 launch sites: 61 `"$WOLF_BIN" &` + 1 `"$app_bin" &`), counts → Eleven guards / `71 passed`. Accepted extensions: any `sleep` (not only SETTLE) between launch and `wait_ready` fails check 3; the next launch line ends a scan. Coder round 2 done: R1 implemented, H1 measured true, full conversion + docs applied. Full smoke run after: `70 passed … finished in 536.49s`, real 8m56.554s (baseline 580.18 s / 578.47 s, real 9m40s / 9m38s): ~43 s saved. SAVEAS gate did not discriminate (E139 on the falsifier's second write), so `DIALOG_SETTLE` stays at that site with a comment. Remaining: test-writer (unit tests, guard, counts) → coder (run them, falsifiers E1, E2, E4 + guard and unit falsifiers, flake loops, gate) → reviewer → technical writer.

### Evidence so far
- E3 (= H1(a), priming removed): `wolf-ready: waiting on no-keyboard`, exit 40, `finished in 32.71s`.
- H1(b): with priming, `kbd_devices=1 has_keyboard=true live_wtype_procs=0` → `wolf-ready: wrote …`.
- H1(c): `window_renders_typed_text_under_cage --exact` 20/20, 2.85–2.94 s each.
- E5 Save: the documented revert → `timed out waiting for svromeo.md to contain zzsavedelta` + the Rust assertion names the failure.

### R1 amendments from coder round 2
- `has_keyboard` reads `!seat.devices(gdk::SeatCapabilities::KEYBOARD).is_empty()`, not `Seat::keyboard()`/`capabilities()`: GDK returns the logical "Core Keyboard" and `KEYBOARD` caps even with zero physical keyboards (measured `keyboard=Some("Core Keyboard") caps=POINTER | KEYBOARD kbd_devices=0`).
- Trigger: `seat.connect_device_added`, gated on `ready_armed`; the `notify::is-active` trigger is removed.
- The guard's "no old probe" check matches the exact old loop line `if wtype "" >/dev/null 2>&1; then` (`wtype ""` legitimately remains in the priming code).

## Revision R1 (orchestrator decision, from coder measurements)

Measured by the coder (10-run loops and manual cage runs; all experiments reverted):
- Under headless cage, `gtk::Window::is_active()` becomes true only after a virtual keyboard sends a real key event; `wtype ""` does not activate the window. The plan's `active` conjunct deadlocks: `wolf-ready: waiting on not-active` for 30 s, exit 40.
- The `wtype ""` poll is NOT vacuous. It creates a virtual keyboard, so the seat gains keyboard capability. Without it, the first key is lost in 5/10 runs (signature (c): the leading `i` dropped). With `active` off and one `wtype ""` at the start of `wait_ready`: 10/10 pass.

Decision:
1. Replace `Blocker::NotActive` with `Blocker::NoKeyboard` (label `no-keyboard`). `WindowReadiness.active` becomes `has_keyboard: bool`, read as "this window's display's default seat exposes a keyboard device" (`gdk::Display::default_seat()` → `Seat::keyboard()` is `Some`, or the gtk4-rs equivalent). It stays an input-window conjunct, checked last. Rationale: the observed loss is a seat-capability race — a key sent before the client has bound `wl_keyboard` has no resource to land on. The conjunct is the client-side half of that race; the priming is the server-side half.
2. `wait_ready` runs one `wtype ""` before polling, with its own failure message (wtype missing, or cage not up yet; retry it inside the poll loop until it succeeds). It sends no key event.
3. Before converting the remaining scenarios, the coder must measure hypothesis H1: (a) without the priming, the predicate times out naming `no-keyboard` (this becomes falsifier E3, replacing the old one); (b) with the priming, `has_keyboard` turns true and stays true after `wtype ""` exits; (c) `window_renders_typed_text_under_cage --exact` passes 20/20. If H1(a) or H1(b) is false, stop and report; do not fall back to a sleep.
4. Accepted from the coder: `wait_ready` kills the app (`kill "$1"`) before `exit 40` on timeout, so a red run fails in ~30 s instead of hanging cage; `#[must_use]` removed from `launch_readiness` (clippy `double_must_use`).
5. Docs and comments: the plan's claim that the `wtype ""` poll is vacuous is wrong. Rewrite the relevant comments and docs to say it primes the seat's keyboard capability, and that `wait_ready` now does that.
6. Test-writer changes: T2/T4/T13/T14 rows use `NoKeyboard`/`has_keyboard`/`no-keyboard`; `ALL` stays 10 entries. The guard matches a definition only where the trimmed line starts with `wait_ready() {` (the pointer helper `hwait_ready() {` contains the substring).

## Measurement isolation

The harness removes `DBUS_SESSION_BUS_ADDRESS`, but GIO falls back to `$XDG_RUNTIME_DIR/bus`. While the user's own packaged wolf-gnome was running, every smoke launch handed off to it as a single-instance activation; the first baseline attempt failed every test and left windows in the user's instance. All measurements in this increment run with `XDG_RUNTIME_DIR` set to a private directory on the command line (e.g. `XDG_RUNTIME_DIR=/tmp/claude-1000/wolf-xdg`). Fixing the gap in the harness is out of scope here (it belongs with the private-runtime-dir work in step 2) and has been reported to the user.

## Problem statement

The 60 `*_under_cage` smokes in `crates/wolf-gnome/tests/smoke_headless.rs` took 576 s of a 676 s `cargo test --workspace` run. They serialize on `CAGE_LOCK`; each launches cage + wolf-gnome + captive nvim + WebKit through the single helper `cage_command`, runs a `sh -c` scenario, then grim + tesseract OCR.

Most of that time is fixed sleeps in the scenario scripts: 291 `sleep "$SETTLE"` (SETTLE=1.0, 55 copies of the knob; canonical comment at the `SCENARIO` const ~line 2603), plus PREVIEW_SETTLE=2.0, TREE_SETTLE=1.5, DRAWER_SETTLE=2.0, TABS_BAR_SETTLE=2.0, TOGGLE/PASTE/DIALOG_SETTLE=2.0, RESTORE_SETTLE, MRU_GAP, COMPOSE_STEP, POINTER_STEP.

Goal: replace fixed sleeps with waits on the condition the next step actually depends on, cutting suite time without adding flakiness, and making the smokes tolerant of a loaded machine (so a later, out-of-scope step can run cages in parallel).

### Known facts

- The first SETTLE is the only budget before the first keystroke. The existing `wtype ""` poll is vacuous: cage advertises the virtual keyboard before any surface maps. SETTLE was measured (warm shader cache threshold 0.25 s, cold 0.35 s, chosen 1.0). Failure signature (a) in `window_renders_typed_text_under_cage` documents what lost early keystrokes look like.
- Many smokes have the captive Neovim write "readback" files (getcwd(), bufname, winwidth, …, via typed `:call writefile(...)`) under XDG_RUNTIME_DIR, then sleep, then the Rust side reads them. Those sleeps can become polls for the file with a generous timeout.
- No app-side readiness hook exists. A test-only signal (e.g. an env var naming a file the app touches once the first frame is drawn AND the captive Neovim is attached and accepting input) would replace the first SETTLE. Open question: whether per-surface readiness (WebKit preview loaded, drawer, tree, tab bar) is needed for the other settles.
- Many settle values carry measured-flake history in their comments. They must not simply be shrunk. Every replaced wait must become a condition the following step genuinely depends on, with a timeout that fails loudly and names the condition.

### Scope and increments

- Increment 1 (this run): the app-side readiness signal; replace the initial SETTLE across all scenarios; convert readback-file sleeps to polls.
- Later increments (not this run unless the plan says otherwise): per-surface settles (preview, drawer, tree, tabs, dialogs, paste, pointer steps).
- Out of scope: parallel cages, private XDG_RUNTIME_DIR per cage, replacing CAGE_LOCK.

### Requirements

- Measure before/after wall time of the smoke binary: `nix develop --command cargo test -p wolf-gnome --test smoke_headless`.
- Run the converted smokes repeatedly (at least 5x) to check flakiness.
- Any app-side hook is documented, off by default, and costs nothing when unset (mirror `WOLF_FRAME_STATS`).
- Docs per CLAUDE.md: update `crates/wolf-gnome/DECISIONS.md`, `src/README.md` and `tests/README.md` entries in place (net-negative where possible), present tense only, prettier after editing Markdown.
- Final check: the Linux verification gate in `build.md` passes.
- Do not commit.

---

# Architect plan — Increment 1: app-side readiness signal + file waits replace fixed launch sleeps

## 1. Summary

A test-only, off-by-default hook `WOLF_READY_FILE=<path>`: the app writes its pid to that path once its launch windows accept keystrokes. A shared shell prelude (`cage_command` prepends it to every scenario) provides `wait_ready <pid> <died-code> <label>`, which polls for the file to name the launched pid (30 s, then exit 40 naming the condition). In every scenario, the vacuous `wtype ""` loop / `wait_window` call plus the initial `sleep "$SETTLE"` after each launch (62 launch sites + 1 twin-launch site) becomes one `wait_ready`; the 5 run-2 `sleep "$RESTORE_SETTLE"` lines go too, since "restore finished" is a conjunct. Two sleeps waiting for a file Neovim writes (end of the Save smoke and the Save As dialog smoke) become content polls.

Investigation finding: every typed `:call writefile(...)` readback already uses `wait_file` (59 call sites, 32 copies of the function); no `sleep "$SETTLE"` is a readback wait, so item (b) reduces to the two save-file waits. The remaining 227 `sleep "$SETTLE"` are inter-step waits for something else and stay (classified in §3). Expected saving this increment: ~45–60 s.

### What "ready to accept keystrokes" means (evidence)

- Core attached: `WolfCore::start` (`crates/wolf-core/src/api.rs:237-359`) blocks inside `window::build` (`window.rs:434`) until nvim is spawned, `ui_attach`ed and every `setup_*` RPC has answered; the input pump is an unbounded FIFO (`api.rs:370-425`), so once `build` returns, a key that reaches GTK reaches nvim.
- Key path wired: `wire_keys` (`window.rs:814`, `im.focus_in()` `:951`) and the focus grab (`:852`) run synchronously in `build`. `GTK_IM_MODULE` is pinned to the simple context (`smoke_headless.rs:11429`), so no async IM dependency.
- The missing precondition is the compositor's. Signature (a) (`smoke_headless.rs:15796-15807`) is keys sent before the toplevel is mapped and keyboard-focused, which cage drops before GTK sees them. cage gives keyboard focus to the newest mapped view and activates it, which GTK reports as `gtk::Window::is_active()`. The file is written only after the app processed that activation, and wtype starts only after the file exists, so cage processes wtype's keys after routing focus to the window. `is_active` of the last-mapped launch window is the delivery conjunct.
- Panes exist and are sized: panes appear on the first wake with a window grid (`pane_view.rs:629`); grid sizes follow the allocation (`pending_resize` `:1501`, `on_pane_resize` `:2523`). Pointer smokes click at derived pixels straight after launch and several readbacks read `winwidth`, so the last-mapped window must have ≥1 bound pane and every bound pane's `requested_grid` (`:296`) must equal the snapshot grid's `cols`/`rows`.
- Restore finished: tab 0's saved layout is consumed on a later wake (`consume_pending_layout` `:665`) and tabs 1..n are created on later wakes (`tab_view.rs:475`), both with blocking main-thread RPCs that can interleave with typed keys. Every launch window needs `pending_layout == None` and no restore queue still owing pages (an abandoned restore, `may_create == false`, `tab_view.rs:172-194`, counts as finished).
- Multi-window restore (two-window smoke run 2): restore conjuncts apply to all launch windows (wake-driven); input conjuncts (focus widget, grids sized, active) apply only to the window whose first frame committed last, because cage maps and focuses it last. That is not necessarily the last-built one (the two-window smoke records that focus is not fixed). An older covered window may get no frame callbacks (inferred from wlroots occlusion, not measured), so requiring it be sized could hang.
- First frame drawn: each launch window must have committed ≥1 frame (frame-clock `after-paint`); the ordinal of the first after-paint is the map order, since all windows share one `wl_display` and cage processes their first commits in that order.
- Rejected conjunct, "Neovim not at a prompt": the signal means "keys are delivered", not "interpreted as intended"; the fresh-cwd rule prevents the swap modal, and signature (c) must stay visible as a startup bug.
- Once per process: fires once after `restore_session`, never re-armed. Neovim restart readiness and mid-run New Window are later increments (C6, C7).
- No settle after `wait_ready`: every delivery precondition is a conjunct. If a first keystroke is still lost, a conjunct is missing; a sleep would hide it.

## 2. Files to create

### `crates/wolf-gnome/src/ready_seam.rs` (new; pure — no GTK, no env reads, no clock)

Coder writes the non-test part; test-writer appends `#[cfg(test)] mod tests`.

- `pub const READY_FILE_ENV: &str = "WOLF_READY_FILE";`
- `pub const LINE_PREFIX: &str = "wolf-ready: ";` (stderr prefix)
- `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct PaneReadiness { pub bound: usize, pub sized: usize, pub layout_pending: bool }`
- `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct WindowReadiness { pub liveness: Option<wolf_core::Liveness>, pub panes: Option<PaneReadiness> /* None = no selected tab */, pub restore_owed: bool, pub first_paint: Option<u32>, pub active: bool, pub has_focus_widget: bool }`
- `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Blocker { NoLaunchWindow, CoreNotAlive, NoSelectedTab, NoPaneBound, LayoutPending, RestoreOwed, NotPainted, NoFocusWidget, GridsUnsized, NotActive }` with `pub const ALL: [Blocker; 10]` and `pub fn label(self) -> &'static str` (labels: `no-launch-window`, `core-not-alive`, `no-selected-tab`, `no-pane-bound`, `layout-pending`, `restore-owed`, `not-painted`, `no-focus-widget`, `grids-unsized`, `not-active`).
- `#[must_use] pub fn launch_readiness(windows: &[WindowReadiness]) -> Result<(), Blocker>` — fixed check order (tests pin it):
  1. empty slice → `NoLaunchWindow`;
  2. for each window in slice order: `liveness == Some(Alive)` else `CoreNotAlive` (exhaustive match over `Option<Liveness>`, no `_` arm); `panes` is `Some` else `NoSelectedTab`; `bound >= 1` else `NoPaneBound`; `!layout_pending` else `LayoutPending`; `!restore_owed` else `RestoreOwed`; `first_paint.is_some()` else `NotPainted`;
  3. the input window is the one with the greatest `first_paint`; it must have `has_focus_widget` (else `NoFocusWidget`), `sized == bound` (else `GridsUnsized`), `active` (else `NotActive`), in that order;
  4. `Ok(())`.
- `#[must_use] pub fn grid_settled(requested: Option<(u16, u16)>, actual: Option<(usize, usize)>) -> bool` — true only when both `Some` and cols and rows are equal (`usize::from`, not `as`).
- `#[must_use] pub fn restore_owed(queued: usize, may_create: bool) -> bool` — `queued > 0 && may_create`.
- `#[must_use] pub fn ready_file_path(raw: Option<&str>) -> Option<&str>` — the same trim rule as `frame_stats::report_file_path`.
- `#[must_use] pub fn ready_file_contents(pid: u32) -> String` — `format!("{pid}\n")`.
- `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum ReadyStep { Write, Report(Blocker), Wait }` and `#[must_use] pub fn next_step(last: Option<Blocker>, now: Result<(), Blocker>) -> ReadyStep`: `Ok` → `Write`; `Err(b)` with `Some(b) != last` → `Report(b)`; otherwise `Wait`.

Module doc: one short block with each conjunct's reason (as above), why older windows need no sizing, and why there is no post-ready margin. No other new files (unit tests live in the seam; the guard goes in `smoke_headless.rs`).

## 3. Files to modify

### `crates/wolf-gnome/src/lib.rs`
`pub mod ready_seam;` (alphabetical, before `refresh_coalescer`) and one bullet in the crate doc's seam list.

### `crates/wolf-gnome/src/app.rs`
`pub(crate) fn ready_file_path() -> Option<PathBuf>`: `std::env::var(ready_seam::READY_FILE_ENV)` through `ready_seam::ready_file_path`. Glue only, shaped like `init_lua_path` (`:148`).

### `crates/wolf-gnome/src/pane_view.rs`
`PaneHandle::readiness(&self) -> ready_seam::PaneReadiness` next to `snapshot_readiness` (`:3452`), delegating to a new `PaneInner::readiness`. Borrow discipline, four separate statements: (1) `layout_pending = self.pending_layout.borrow().is_some()`; (2) collect `(grid, requested_grid)` for panes whose `grid` is `Some`; (3) read the snapshot under its own core borrow and count pairs where `ready_seam::grid_settled(req, snap.grid(g).map(|g| (g.cols, g.rows)))`; (4) return `PaneReadiness { bound: pairs.len(), sized, layout_pending }`. Nothing else changes.

### `crates/wolf-gnome/src/coordinator.rs`
- Private `struct ReadyProbe { path: PathBuf, launch: Option<Vec<WindowId>>, last: Option<Blocker>, fired: bool, next_paint: u32 }`; field `ready: RefCell<Option<ReadyProbe>>` on `CoordinatorInner` (`:91-108`), set in `build()` (`:122`) from `app::ready_file_path()`.
- `pub fn ready_pending(&self) -> bool` (probe exists and has not fired); `pub fn next_paint_ordinal(&self) -> u32` (increments `next_paint`).
- `pub fn check_ready(&self)`:
  1. in its own statement, clone `(path, launch)` out if the probe is pending and `launch` is `Some`; otherwise return;
  2. build one `WindowReadiness` per launch id from `self.handles()` (a missing handle → `liveness: None` and defaults);
  3. `launch_readiness`;
  4. in one `borrow_mut` statement apply `next_step` (`Report(b)` → set `last`; `Write` → `fired = true`);
  5. after the borrow is released, do the I/O. `Report` → stderr `"{LINE_PREFIX}waiting on {label}\n"` via `write_all`, result discarded (the frame-metrics rule: never `eprintln!` on a diagnostic path). `Write` → `ready_file_contents(std::process::id())` to `<path>.tmp` (append `.tmp` to the `OsString`), `std::fs::rename` to `path`, then `"{LINE_PREFIX}wrote {path}\n"`; on error print `could not write …` once and stay fired.
- Private `fn arm_ready(&self, launch: Vec<WindowId>)`.
- `open_window` (`:327`) returns `Option<WindowId>` (`.map(|h| h.id())`); `open_new_window` (`:311`) uses `.is_none()`.
- `restore_session` (`:258`) collects opened ids, then `self.arm_ready(ids); self.check_ready();`.
- Module doc: a sixth bullet, "the test-only readiness probe — process-scoped because its condition spans every window the launch restores".

### `crates/wolf-gnome/src/window.rs`
- `WindowInner` (`:273`): add `core: Rc<RefCell<Option<WolfCore>>>` and `first_paint: Cell<Option<u32>>`; update its doc ("everything the coordinator needs to ask a window for").
- `pub fn readiness(&self) -> ready_seam::WindowReadiness` on `WindowHandle`, each read its own statement: `liveness` = `core.borrow().as_ref().map(WolfCore::liveness)`; `panes` = `tabs.selected().map(|t| t.panes().readiness())`; `restore_owed` = `tabs.pending_restore().is_some_and(|r| ready_seam::restore_owed(r.layouts.len(), r.may_create))`; `first_paint` = `first_paint.get()`; `active` = `window.is_active()`; `has_focus_widget` = the window's focus widget is `Some` (`GtkWindowExt::focus` or `RootExt::focus`, whichever gtk4-rs exposes).
- In `build`: at the top, `let ready_armed = coordinator.ready_pending();` and `let coordinator_weak = coordinator.weak();`. In the drain-loop future (`:753-783`), after `frame_metrics::maybe_emit()`: `if ready_armed { if let Some(c) = coordinator::upgrade(&coordinator_weak) { c.check_ready(); } }`. After `window.present()` and the focus grab (`:849-854`), if `ready_armed`, call a new private `fn wire_ready_triggers(handle: &WindowHandle, coordinator: Weak<CoordinatorInner>)` that connects `window.connect_is_active_notify(→ check_ready)` and `window.frame_clock()`'s `connect_after_paint` (first sets `first_paint` from `coordinator.next_paint_ordinal()` if `None`, then `check_ready`). Both closures hold `Weak<WindowInner>` and `Weak<CoordinatorInner>`. If `frame_clock()` is `None`, print one message.
- When unset, nothing is connected: one env read per process, one bool test per wake.
- No `tab_view.rs` changes: `selected()` (`:802`), `pending_restore()` (`:836`) and `TabHandle::panes()` (`:252`) already exist.

### `crates/wolf-gnome/tests/smoke_headless.rs` (coder: harness + scenarios; test-writer: guard + guard counts)

**Harness**
- `const READINESS_PRELUDE: &str` (shell) defining `READY_TRIES=600`, `READY_INTERVAL=0.05` (30 s), `READY_TIMEOUT_EXIT=40`, and:
  - `wait_ready()`, with only `_ready_`-prefixed variables so it cannot clobber a scenario's `tries`/`t`. Each iteration: `kill -0 $1` failing → echo "`$3` (pid `$1`) exited before it was ready for input" and `exit "$2"`; `_ready_got=; read -r _ready_got < "$WOLF_READY_FILE" 2>/dev/null || true`; equal to `$1` → return 0; otherwise sleep. On timeout: echo a line naming `$WOLF_READY_FILE`, the pid and the budget, plus "the app's `wolf-ready:` stderr lines name the last unmet condition", then `exit "$READY_TIMEOUT_EXIT"`.
  - `wait_file_contains()` (`<file> <needle> <label>`), 200 × 0.1 s: return 0 when `[ -s file ] && grep -qF needle file`; on timeout echo "timed out waiting for `<file>` to contain `<needle>`" and return 1.
- `fn smoke_ready_file() -> PathBuf`: `std::env::temp_dir().join(format!("wolf-smoke-ready-{pid}-{n}"))` with `n` from a `static AtomicUsize`; remove a stale file and its `.tmp`. Not a sibling of the cwd (the typed-`:cd` smoke, `:30620-30689`, re-roots the sidebar to `cwd.parent()`); the prefix must not start with `TEMP_CWD_PREFIX`.
- `cage_command` (`:11553`): `.arg(format!("{READINESS_PRELUDE}\n{scenario}"))` and `.env("WOLF_READY_FILE", smoke_ready_file())`. Do not touch `CAGE_LOCK`, `XDG_RUNTIME_DIR` or any other variable. Update its doc (six variables → seven, a bullet for `WOLF_READY_FILE` and the prelude), the guard doc and message (`:11588-11709`, incl. "ALL SIX" at `:11552`, plus a clause that a bypassing smoke loses `wait_ready` (exit 127) and `WOLF_READY_FILE` (`set -u` fails)), and `packaged_cage_command`'s "None of those six" (`:37095-37116`).

**Scenario conversions — category A, 63 sites**
- At each launch, delete the `wtype ""` poll loop or `wait_window N` call and its comment, and the following `sleep "$SETTLE"` with its "Final settle…" comment. Insert `wait_ready "$app_pid" <that site's existing died-early code> "<that scenario's echo label>"`.
- Delete every `wait_window()` definition, every `WAIT_TRIES=75`, and each "SETTLE is the ONLY real budget…" knob comment. Keep `WAIT_INTERVAL` (used by `wait_file`, quit polls, concatenated helpers) and `SETTLE=1.0` with a one-line comment pointing to `SCENARIO`. Re-word COLON_QA's "Sized like WAIT_TRIES" comment (`:4372-4374`).
- The 62 post-launch lines: 2675, 2752, 2838, 2933, 2992, 3074, 3156, 3264, 3352, 3466, 3524, 3619, 3760, 3943, 4125, 4293, 4399, 4496, 5009, 5284, 5661, 6498, 6578, 6710, 6766, 6962, 7049, 7560, 7640, 7755, 8029, 8232, 8471, 8682, 8885, 9160, 9442, 10014, 10683, 22888, 22907, 23241, 23260, 24511, 25520, 26295, 27135, 28223, 28566, 31217, 31286, 31353, 31980, 32690, 32957, 34484, 34885, 35299, 35752, 36279, 38101, 38773. The 63rd: the two-window run-2 `sleep "$SETTLE"` at 8238, after `wait_nvim_count`.
- Delete `sleep "$RESTORE_SETTLE"` at 6581, 6767, 7052, 7641, 8233. Keep 8059 (a mid-run New Window, which no readiness file covers).
- Remove `.env("RESTORE_SETTLE", RELAUNCH_RESTORE_SETTLE)` at 21485, 21742, 22280, 29319; keep 22593 (two-window). Rewrite the `RELAUNCH_RESTORE_SETTLE` doc (`:6341-6350`) to say it serves only the mid-run window, and the RESTORE_TABS comments mentioning it (`:6907-6913`, `:7063-7068`).

**Category B**
- SAVE (`:34540-34543`): `sleep "$SETTLE"` → `wait_file_contains "$SAVE_FILE" "$SAVE_SENTINEL" "wolf-gnome save smoke" || true`. A timeout falls through on purpose so the Rust assertion at `:34696` names the red (precedent: the RESTORE_TABS non-fatal readback). Premise: Neovim writes a small fixture (<8 KB) in one write.
- SAVEAS (`:34943-34946`): the final `sleep "$DIALOG_SETTLE"` → `wait_file_contains "$SAVEAS_TYPED_NAME" "$SAVEAS_SENTINEL" "wolf-gnome save-as smoke" || true`. Gate: run the smoke's negative "proposed name not written" falsifier (`:35148`). If that assertion no longer reddens, restore `DIALOG_SETTLE` there and record why.

**Comments that become false — rewrite in place, never append**
- Module doc "A vacuously-passing check recorded here" (`:365-401`): delete; add a short "Readiness before the first input" section (`wait_ready`, `WOLF_READY_FILE`, exit 40, `wolf-ready:` stderr).
- `SCENARIO`'s doc bullets (`:2582-2600`) and its canonical in-script SETTLE block with the margin table (`:2606-2646`, `:2652-2660`, `:2674`): SETTLE is the inter-step settle, not the launch budget.
- ~20 doc comments "poll until cage is up (NOT until the window maps — see SCENARIO's SETTLE comment)" (e.g. `:2706, 2791, 2886, 2948, 3030, 3112, 3220, 3307, 3417, 3480, 3574, 3685, 4249, 4337, 4452, 6418, 7714, 8626, 8827`).
- POINTER_PREAMBLE (`:5353, 5395-5403, 5640-5643, 5659-5660`), the frame-stats scenario (`:36239-36245, 36260-36264, 36278`), and the two-window comments (`:7970-7974, 7990-7992`).
- Every "died before its window mapped" → "died before it was ready for input" (exit tables, ~40 occurrences from `:3707` to `:38734`, plus the shell echo lines).
- `window_renders_typed_text_under_cage` signature (a) (`:15796-15807`): it means the readiness predicate missed a conjunct; read the `wolf-ready:` stderr and DECISIONS "The readiness file"; a post-ready sleep is not the fix. Rewrite (c)'s last sentence (`:15829-15831`) to match.
- `SHARED_SHADER_CACHE_DIR` / `smoke_shader_cache_dir` docs (`:11216-11218, 11250-11290, 11324-11328`) and panic text (`:11334-11338`); the `cage_command` XDG_CACHE_HOME bullet (`:11464-11474`); guard paragraph 3 (`:11614-11619`). New point: a cold cache costs time in `wait_ready`, not correctness. Do not change XDG_CACHE_HOME behaviour.
- Final check, must return nothing: `rg -n 'ONLY real budget|wtype ""|margin table|WAIT_TRIES|before its window mapped' crates/wolf-gnome/tests/smoke_headless.rs`.

### Classification of the 291 `sleep "$SETTLE"` uses

| Category | Uses | Disposition |
| --- | ---: | --- |
| A. Pre-first-keystroke budget (62 post-launch + 8238) | 63 | This increment |
| B. Wait for a file Neovim writes (34541; plus the SAVEAS `DIALOG_SETTLE`, not counted) | 1 | This increment |
| C1. Neovim-only keystroke steps (Ex, normal/insert keys, `fill_buffer` ×3); next step is FIFO-ordered behind them, most deletable once each next step is checked to read only Neovim state | 85 | Later |
| C2. Pane layout changes (`<C-w>v/s/h/c/j/k`, `:vsplit`/`:split`, drawer focus); needs a "layout settled, grids sized" signal | 31 | Later |
| C3. Overlay and app-action chords (open, query ranking, activation, New buffer, Alt+Left, re-root); needs per-overlay readiness | 48 | Later |
| C4. Pre-capture settles before `grim`; needs "frame presented after the last change" | 36 | Later |
| C5. Pointer steps after `hsend`/`hstep` | 16 | Later |
| C6. Crash/restart recovery (5114, 5153, 6087); could reuse the predicate, re-armed | 3 | Later |
| C7. Other: mid-run New Window 8058, clipboard copy 28597, save-dialog typed name 34940, compose/IM chords 35809/35821/35832/35841, frame-stats idle 36297 | 8 | Later |

Total 63 + 1 + 227 = 291. All other named settles stay (`PREVIEW_`, `TREE_`, `DRAWER_`, `TABS_BAR_`, `TOGGLE_`, `PASTE_`, the remaining `DIALOG_`, `MRU_GAP`, `COMPOSE_STEP`, `POINTER_STEP`, `BURST_GAP`, `RESTORE_SETTLE` at 8059).

Shared helpers: new helpers live in one `READINESS_PRELUDE` that `cage_command` prepends, so no scenario can forget them. The 32 existing `wait_file` copies are correct polls with scenario-specific messages; leave them (later increment).

### Documentation (coder; in place, present tense, then `prettier --write` and `./tools/lint/markdown_check.sh <files>`)

- `crates/wolf-gnome/DECISIONS.md`: a new `## The readiness file` section after "### The measured reading", before "## Present limitations", about four paragraphs — purpose (test-only, off by default; launch-to-input is 0.25–0.35 s on the dev host and longer under load, so no fixed sleep is a sound budget); the predicate conjuncts with reasons (`active` is the delivery precondition because cage activates exactly the window it focuses; the input window is the last-mapped, by first-commit order; older windows need not be sized, marked as inferred; restore conjuncts cover the blocking RPCs that interleave with typed keys); triggers (end of each wake, `notify::is-active`, frame-clock after-paint; wired only when armed; no timer); lifetime and format (once per process, not re-armed on restart; holds the pid, atomic rename, so a scenario's two launches can share one path; coordinator-owned, so the process-global carve-out stays single). Also: "The coordinator and its windows" ¶1 "five things" → six; "Threading and the wake" "Nothing polls…" sentence adds the readiness probe beside the frame-time instrument; "The headless smoke test" "Three details" → four, adding that no scenario sends input before the readiness file names its pid, with a link.
- `crates/wolf-gnome/src/README.md`: a new `#### ready_seam.rs` entry before `refresh_coalescer.rs` (≤200 words); `app.rs` Public API + `ready_file_path`; `coordinator.rs` Public API + `ready_pending`, `next_paint_ordinal`, `check_ready` and one Decisions bullet; `window.rs` + `WindowHandle::readiness` and a Gotcha that triggers are wired only when armed; `pane_view.rs` Public API + `readiness()`.
- `crates/wolf-gnome/README.md` "Running": "Four environment variables" → five, with one sentence: `WOLF_READY_FILE` belongs to the smoke harness; the binary writes its pid there once its launch windows accept input (link to DECISIONS).
- `crates/wolf-gnome/tests/README.md` "Smoke conventions": a new paragraph "**No scenario sends input before `wait_ready`.**" covering the prelude, the pid match, exit 40, the died-early code, `SETTLE` remaining only between steps, no post-ready margin, and the guard's name.
- `build.md`: no change.

## 4. Test plan (test-writer, unless noted)

All auto-discovered (no Bazel target for this crate). Confirm each runs with a non-zero count: `nix develop --command cargo test -p wolf-gnome --lib ready_seam` and `nix develop --command cargo test -p wolf-gnome --test smoke_headless every_launch_waits`.

Unit tests in `src/ready_seam.rs` (table-driven, literals re-typed rather than imported):

| Test | Asserts | Falsifier |
| --- | --- | --- |
| T1 `a_settled_single_window_launch_is_ready` | baseline → Ok | `launch_readiness` always `Err(NotActive)` |
| T2 `each_unmet_condition_names_its_own_blocker` | baseline with one field flipped → expected blocker; rows: liveness None/NotResponding/Dead, panes None, bound 0 & sized 0, layout_pending, restore_owed, first_paint None, focus false, sized < bound, active false | delete that conjunct; named case: drop `bound >= 1` and the zero-pane row answers Ok (0 == 0) |
| T3 `only_the_last_mapped_window_must_accept_input` | w0 (painted 2, active) + w1 (painted 1, inactive, unsized, no focus) → Ok | pick `windows.last()` not max `first_paint`, or apply input checks to every window |
| T4 `the_last_mapped_window_must_be_active` | w0 (1, active), w1 (2, inactive) → NotActive | `any(active)` |
| T5 `every_launch_window_must_finish_its_restore` | older window `layout_pending` → LayoutPending | settle checks on the input window only |
| T6 `an_empty_launch_is_never_ready` | `[]` → NoLaunchWindow | remove the empty arm |
| T7 `an_unpainted_window_holds_readiness` | `[None, Some(1) active]` → NotPainted | skip `None` when picking the max |
| T8 `grid_settled_table` | both missing cases, equal, col mismatch, row mismatch | `requested.is_some()` alone; or cols only |
| T9 `restore_owed_table` | (0,t),(0,f),(2,t),(2,f) | ignore `may_create` ((2,f) fires; an abandoned restore blocks forever) |
| T10 `ready_file_path_table` | `report_file_path`'s rows | return untrimmed |
| T11 `ready_file_contents_is_the_pid_and_one_newline` | `"4242\n"` | drop the newline or add a prefix |
| T12 `next_step_table` | five rows | always `Report` (same-blocker row fires: a stderr line per frame) |
| T13 `blocker_labels_are_distinct_and_nonempty` | over `ALL`, `ALL.len()` pinned to 10 | duplicate a label |
| T14 `the_env_var_and_prefix_are_the_harness_contract` | literals `"WOLF_READY_FILE"`, `"wolf-ready: "` | rename either |

Guard in `tests/smoke_headless.rs`: `every_launch_waits_for_readiness_before_its_first_input`. Pure (`include_str!`, no `CAGE_LOCK`, no env), placed next to `cage_is_only_spawned_through_the_isolating_helper`, with needles built from `concat!` fragments as `CAGE_SPAWN_NEEDLE` (`:11586`) is.
- A launch line: trimmed, with `" &"` stripped, starts with `"$`, ends with `"`, contains no space.
- For each launch line, scan forward to the first line whose trimmed form starts with `wtype`, `hsend`, `hstep` or `grim`, or that ends the raw string (`"#;`). Assert a line starting with `wait_ready "$app_pid"` occurs in that window, and no `sleep "$SETTLE"` occurs between the launch and that `wait_ready`.
- Assert ≥60 launch sites; exactly one `wait_ready() {` definition in the file; zero occurrences of the `wtype ""` probe line.
- Falsifiers: put `sleep "$SETTLE"` back before `wait_ready` in `PICKER_SCENARIO` (second assertion fires); delete its `wait_ready` (first fires); break the launch matcher (floor fires); paste a second `wait_ready() {` (definition count fires).
- Count updates: module doc ("Ten pure guards" → Eleven, "All ten" → All eleven, "Nine of the ten" → Ten of the eleven, "`70 passed`" → "`71 passed`", plus a bullet for the new guard); `tests/README.md` `smoke_headless.rs` entry ("Ten compositor-free guards … `70 passed`" → "Eleven … `71 passed`").

End-to-end falsifiers on existing smokes (coder runs after implementation, reverts each, records the outcome):
- E1 never fires: `check_ready`'s `Write` arm skips the write → `window_renders_typed_text_under_cage` fails after ~30 s with cage status 40; stderr has the timeout line and either `wolf-ready: waiting on …` or no lines.
- E2 fires too early: write the ready file as the first statement of `restore_session` (before any window is built) and mark fired → signature (a) (row 0 blank, cursor on line 2) in ≥4 of 5 runs.
- E3 `active` alone: skip the `NotActive` check, run `window_renders_typed_text_under_cage` ×10. If green, record in `tests/README.md` "What is still unproven" that this conjunct's only falsifier is its unit row.
- E4 restore conjuncts: skip the `LayoutPending` and `RestoreOwed` checks; run `pane_layout_survives_relaunch_under_cage` and `three_tabs_survive_a_relaunch_under_cage` ×5. Expected red on the first; record either way.
- E5 save conversions: run each save smoke's documented revert once; confirm the Rust message still names the failure and the SAVEAS negative assertion still discriminates, else revert that conversion as in §3.

## 5. Order and split

1. Coder: `ready_seam.rs` (non-test) + `lib.rs` mod line; build.
2. Coder: `app.rs`, `pane_view.rs`, `coordinator.rs`, `window.rs`; build; `cargo clippy -p wolf-gnome --all-targets -- -D warnings`.
3. Coder: harness (`READINESS_PRELUDE`, `smoke_ready_file`, `cage_command`).
4. Coder: convert SCENARIO only; run `window_renders_typed_text_under_cage` once; confirm a `wolf-ready: wrote` line and a pass. Stop and investigate on a timeout (the blocker label names the cause; `not-active` or `grids-unsized` means an assumption in §1 is wrong).
5. Coder: remaining scenarios, RESTORE_SETTLE sites, B, then the comment rewrites.
6. Coder: docs.
7. Test-writer, after the coder (both edit the module doc): `ready_seam.rs` unit tests, the guard, the guard-count edits.
8. Falsifiers E1–E5, measurement, flake loops, gate.

## 6. Measurement protocol

- Pre-flight before every run: `pgrep -a cage` prints nothing and no other `cargo test` is running (overlap causes cage lockfile collisions).
- Baseline, before any edit, run twice: `nix develop --command bash -c 'cargo test -p wolf-gnome --test smoke_headless --no-run && time cargo test -p wolf-gnome --test smoke_headless'`. Record libtest's "finished in" and `real`.
- After: the same two runs. Expected saving ~45–60 s (63 × (1.0 s − ready latency) + 5 × 2.0 s + ~3 s). Report the mean delta honestly.
- Flake checks: 5 consecutive full runs (each its own `nix develop --command`), all `71 passed`; `window_renders_typed_text_under_cage --exact` ×20; the five relaunch smokes and the two save smokes ×5 each.
- Informative only: `window_renders_typed_text_under_cage` ×10 under `nproc` CPU busy-loops (not another `cargo test`), to sanity-check the 30 s budget.
- Finally the Linux verification gate (`build.md:146-159`).
- The three `#[ignore]`d packaged smokes build from git-tracked sources, so an untracked `ready_seam.rs` is invisible to them; they are outside the gate. Do not stage or commit to run them.

## 7. Timeouts

- `wait_ready`: 600 × 0.05 s = 30 s. An idle launch is ready in well under 1 s. Restore adds up to 2T−1 blocking RPCs, each bounded by the 5 s `RPC_TIMEOUT_DEFAULT` (`api.rs:54`); a timeout marks the core NotResponding, which makes readiness unreachable, so a wedge fails loudly. At 8 parallel cages, ~8× slower startup is still ~7× inside the budget. A red run is capped at 30 s instead of hanging. The 50 ms interval costs ~25 ms per launch on average.
- `wait_file_contains`: 200 × 0.1 s = 20 s, matching `FILE_TRIES × WAIT_INTERVAL`.
- Every timeout names its condition: the shell line names the file, pid or needle; the app's `wolf-ready: waiting on <label>` names the conjunct.

## 8. Dependencies and lint

No new dependencies: `std` (`fs`, `sync::atomic`) and existing gtk4/gdk4 API (`connect_is_active_notify`, `frame_clock`, `FrameClock::connect_after_paint`, `Window::focus`). No `#[allow]` expected: `usize::from(u16)` rather than `as`; exhaustive `Option<Liveness>` match with no `_` arm; `let _ =` on stderr `write_all`; the guard's needles via `concat!` and computed-count assertions (no `assertions_on_constants`). `cargo fmt`; `prettier --write` and `./tools/lint/markdown_check.sh` on the edited Markdown, avoiding banned changelog words and ALL-CAPS runs.
