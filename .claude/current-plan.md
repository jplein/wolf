# Plan: Neovim crash & stall recovery

## FOLLOW-UP BUG (active): restart is invisible — no user indication

Reported repro: `:q!` in the editor. Actual: Neovim silently restarts, no indication.
Expected: an overlay/toast tells the user Neovim was restarted.

Root cause (confirmed by reading Project.swift + NvimStatusOverlayView.swift): on a
`Dead` wake, `Project.onWake()` runs SYNCHRONOUSLY on the main thread —
`showNvimOverlay(.dead)` then `restartNeovim()`, which BLOCKS the main thread through the
entire respawn, then hides the overlay — all in one turn with NO intervening paint cycle,
so the full-bleed overlay never renders. And the fresh nvim's first render wake hits
onWake's Alive (default) branch which calls `hideNvimOverlay()`, so any liveness-driven
overlay is cleared immediately anyway. Net: the restart is invisible. The existing
full-bleed opaque NvimStatusOverlayView is also the WRONG vehicle for a post-restart
confirmation (it would opaquely cover the now-working editor).

Fix direction: add a TRANSIENT, auto-dismissing "Neovim was restarted" notice (a small
banner/toast, NOT the full-bleed overlay) shown from `restartNeovim`'s SUCCESS path, with
its own timer, INDEPENDENT of the liveness-driven overlay show/hide (so onWake's Alive
branch does not clear it). Front-end only (Swift, manual-smoke — no automated UI test per
established convention). Out of scope: whether `:q!` should be remapped so it doesn't kill
the shared nvim at all (separate design question — flag to user, do NOT change here).

## Status: FOLLOW-UP COMPLETE ✅ (restart-indicator banner; NOT committed)

Delivered: new `NvimRestartedBannerView` (transient top-center "Neovim was restarted"
banner, ~4s auto-dismiss, generation-token coalescing, `[weak self]` timer, `hitTest→nil`
click passthrough, dark/light-aware), fanned from `Project.restartNeovim`'s SUCCESS path via
`WolfWindowController.flashRestartNotice()` → `PaneContainerView.flashRestartNotice()`,
INDEPENDENT of the liveness overlay (onWake's Alive branch never clears it). Also flashes on
the manual `.notResponding` button restart. NOT shown on the no-op/race (restart==false) path.
Reviewer APPROVED (no blockers); both nice-to-haves (click passthrough, comment accuracy)
applied. `bazel build //crates/wolf-macos:Wolf` builds, swift-format PASS, wolf-core 29/29 +
wolf-ffi 31/31 + clippy unaffected. Docs synced (wolf-macos README + Sources/README +
architecture.md) + prettier-clean. Manual-smoke before shipping: `:q!` → file reopens +
top-center banner → auto-dismiss ~4s; rapid `:q!` coalesces to one banner.

### (implementation notes below)

### Architect's plan (restart-indicator banner) — authoritative
- NEW `crates/wolf-macos/Sources/WolfMacOS/NvimRestartedBannerView.swift`: small non-modal
  layer-backed banner (NOT full-bleed), top-center, message "Neovim was restarted",
  `visibleDuration ~4s`; `flash()` uses a `flashGeneration` token + `DispatchQueue.main
  .asyncAfter([weak self])` guarded hide (coalesces rapid restarts, no premature/double hide);
  `applyChromeColors()` + `viewDidChangeEffectiveAppearance` for dark/light; unavailable
  `init?(coder:)`. Mirrors NvimStatusOverlayView/EmptyStateView patterns.
- PaneContainerView: add `private let restartBanner` (added ONCE in init, topmost), method
  `flashRestartNotice()`. Do NOT modify `hideNvimOverlay()` — banner stays independent.
- WolfWindowController: `flashRestartNotice()` forwarder → paneContainer.
- Project.restartNeovim: at END of SUCCESS path (after the existing hideNvimOverlay fan-out),
  add `for view in views.allObjects { view.flashRestartNotice() }`. NOT in the no-op/race
  `guard handle.restart() else` branch. Manual `.notResponding` button restart also flashes
  (same success path) — intended. onWake unchanged (Alive branch touches only nvimOverlay).
- Front-end only; NO core/FFI/Lua change; NO automated test (manual-smoke per convention).
- Docs: wolf-macos README + Sources/README (new banner + the overlay-vs-banner distinction),
  architecture.md crash-overlay line. prettier --write.
- Manual-smoke: `:q!` → file reopens + top-center "Neovim was restarted" banner → auto-dismiss
  ~4s; rapid `:q!` twice coalesces to one banner; dark/light re-resolves; multi-tab shows per view.

---

## (prior effort) Status: COMPLETE ✅ (merged from main, commit 423a493)

## Problem statement

When the embedded Neovim crashes or stalls, the Wolf window freezes on its last
painted frame forever and silently drops input. There is no detection, no user
notification, and no recovery. This effort makes the window recover: detect the
failure, tell the user, restart Neovim per-Project, reopen the files that were
open, and recover unsaved edits via Neovim's built-in swap files — all with no
blocking swapfile prompt.

## Agreed design decisions (settled with the user before implementation)

1. **Stall policy:** manual "Restart Neovim" button. NO auto-kill of a
   possibly-slow-but-alive nvim.
2. **Restore fidelity:** Tier 1 — reopen the file-backed buffers that were open
   (+ active buffer per view). Cursor/scroll (Tier 2) and unnamed-buffer
   re-association are OUT of scope this pass.
3. **Unsaved edits:** recover via Neovim's built-in swap files.
   - swapfile ON always.
   - `'directory'` = a durable, Wolf-owned, per-project swap dir resolved like
     `state.rs::state_dir()` (macOS `~/Library/Application Support/Wolf`, Linux
     XDG state dir, `WOLF_STATE_DIR` override) — NOT the ephemeral TempDir.
   - Drop the default leading `.` from `'directory'` so `.swp` files never land
     next to the user's files.
   - Keep Neovim's default `updatecount`/`updatetime`.
   - Make the `SwapExists` autocmd recovery-aware: dead owner → `v:swapchoice="r"`
     (silent recover); live owner → `"e"`. No blocking prompt ever.
4. **Crash (process death):** detected automatically → notify → restart.
   **Stall (not responding):** notify → manual button drives restart.

## Architecture facts (established via codebase exploration)

- One Neovim per Project, shared across all its views/tabs via tabpages
  (architecture.md:65). One WolfCore owns one NvimProcess (api.rs:143) owns one
  child + one io-loop JoinHandle (nvim.rs:117-121). Restart = PER-PROJECT rebuild.
- No crash/stall detection today. The io-loop JoinHandle resolves with a
  LoopError on nvim stdout EOF but nothing watches it (nvim.rs:119); silently
  dropped. stderr is /dev/null (nvim.rs:1112). Only failure Swift sees is
  wolf_ffi_start returning nil at creation. UI freezes on last frame, drops input.
- RPC timeout (WolfCore::block_on_rpc / timeout_block_on, default 5s,
  WOLF_RPC_TIMEOUT_MS) bounds sync RPCs to benign fallbacks — the stall SIGNAL
  source (repeated NvimError::Timeout), not a detector.
- swapfile currently OFF (init.lua:25) + SwapExists autocmd answers "e"
  (init.lua:31). XDG_* dirs under an ephemeral tempfile::TempDir deleted at exit
  (nvim.rs:97-101), recreated fresh each spawn.
- No own buffer-list mirror: full list is on-demand getbufinfo (nvim.rs:254).
  Snapshot has current_file + per-visible-window `buffers` map. state.json
  persists only geometry + per-tab cwd + sidebar visibility (state.rs).
- state.rs already has state_dir() resolving the durable per-platform dir —
  REUSE for the swap dir.

## Scope of work

### 1. Detection (wolf-core)
- Supervisor awaits the io-loop JoinHandle (and/or child.wait()); on resolution
  mark nvim DEAD (new liveness state on WolfCore) + fire a wake. Log the
  LoopError instead of dropping it.
- Stall: escalate repeated NvimError::Timeout into a "not responding" liveness
  state. NO auto-kill on stall.
- Capture nvim stderr (nvim.rs:1112) instead of /dev/null; log it.

### 2. FFI liveness surface (wolf-ffi)
- Health/liveness accessor (enum Alive / NotResponding / Dead) + wake delivery so
  the transition reaches Swift.

### 3. Restart + restore (wolf-core + wolf-ffi)
- Per-Project restart entry point: kill-if-needed + reap old child, respawn
  NvimProcess in the same cwd, re-run ui_attach + every setup_*, re-attach UI at
  current grid size, expose what the front-end needs to rebind each view to a
  fresh tabpage rooted at its cwd.
- Continuously mirror the buflisted file set + each tabpage's active buffer into
  core via a new wolf_* autocmd notification (existing setup_*/wolf_* pattern in
  nvim.rs + init.lua + redraw.rs). Mirrored Rust-side so it survives nvim death.
  On restart, reopen those files per view.

### 4. Swap-based unsaved-edit recovery (init.lua + wolf-core)
- swapfile ON always; `'directory'` = durable per-project swap dir resolved like
  state_dir(); drop leading `.`; default updatecount/updatetime.
- SwapExists autocmd recovery-aware: dead owner → `"r"`, live owner → `"e"`;
  clean up consumed swap after recovery.
- Swap dir STABLE across respawns of the same project (so the fresh nvim finds
  the dead nvim's swap). Wire the swap dir through NvimConfig so durable swap dir
  and ephemeral hermetic XDG isolation coexist.

### Front-end (wolf-macos primary; keep wolf-gnome compiling/consistent)
- New overlay state on PaneContainerView ("Neovim crashed" / "Neovim stopped
  responding") with a "Restart Neovim" button.
- Dead → auto notify + restart. NotResponding → notify + manual button restart.
- On restart completion, rebind each Project view to a fresh tabpage (reuse
  Project.start / bindFirstTabpage / addTabpage) and reopen the Tier-1 files.

## Required tests (real nvim, no mocks)

a. Killing/wedging the captive nvim fires the liveness signal.
b. Restart respawns and reopens the Tier-1 open-file set per view.
c. A file with unsaved edits comes back recovered via silent v:swapchoice="r"
   with NO pending "confirm" toast and no block.
d. SwapExists chooses "e" (not "r") when the owning process is still alive.

## Constraints

- Docs are source of truth — update every affected README.md + architecture.md.
- Prefer end-to-end/integration tests with a REAL Neovim over mocks.
- Markdown: no hard line breaks mid-paragraph; run `prettier --write` on edits.
- Document limits: SIGKILL/OOM skips preserve_exit (loss window = edits since
  last swap sync); unnamed-buffer re-association out of scope (Tier 1 = file-backed).

## Architect's plan (authoritative)

### 1. Liveness state model (wolf-core)

**New file `crates/wolf-core/src/liveness.rs`:**
- `pub enum Liveness { Alive, NotResponding, Dead }` with `to_u8`/`from_u8` (discriminants `0/1/2`) — single source of truth the FFI `liveness` module mirrors.
- Storage: `Arc<std::sync::atomic::AtomicU8>` — written from tokio worker threads (supervisor, and `block_on_rpc`'s timeout path), read from the AppKit main thread via FFI. Lock-free; avoids lock-ordering against the snapshot `RwLock`.
- Transition helpers (free fns taking `&AtomicU8` + `&WakeFn`), each firing the wake ONLY on an actual change:
  - `mark_dead` — unconditional store `Dead`; wake if not already Dead. Dead is terminal (only `reset_alive` leaves it).
  - `mark_not_responding` — `compare_exchange(Alive → NotResponding)` ONLY. Never downgrades `Dead → NotResponding`. Wake on success.
  - `reset_alive` — store `Alive`; wake if changed (used by restart; optionally by a successful RPC — see §11 open decision).
  - `get(&AtomicU8) -> Liveness`.
- Unit tests in-module (see §9).

**On `WolfCore` (api.rs):** add `liveness: Arc<AtomicU8>` and `wake: WakeFn` (clone of the same `registered_wake(token)` given to the `RedrawHandler`). Re-export `Liveness` from `wolf_core` (lib.rs `pub use`).

### 2. Supervisor task + ownership reconciliation (nvim.rs)

Today `NvimProcess { nvim, io_handle, child }` (nvim.rs:117-121); `shutdown` awaits/aborts `io_handle`. Restructure:
- Remove `io_handle` field. `spawn` MOVES the io-loop handle into a supervisor task.
- Add fields: `child` (kept), `shutting_down: Arc<AtomicBool>` (per-process), `supervisor: JoinHandle<()>`, and a clone of `liveness: Arc<AtomicU8>`.
- `spawn(cfg, handler, liveness, wake)` runs in runtime context; `tokio::spawn`s:
  - **Supervisor:** owns moved `io_handle`. `let result = io_handle.await;` (resolves on stdout EOF = death, clean OR crash) → if `shutting_down` return (intentional exit, no crash report) → log any `LoopError` → `liveness::mark_dead(&liveness, &wake)`.
- **Shutdown-vs-crash race:** `shutdown` sets `shutting_down.store(true)` FIRST, then `qa!`; supervisor no-ops. `shutdown` no longer awaits `io_handle` (supervisor owns it); reaps via existing `child.wait()` timeout + `start_kill()`; then `supervisor.abort()`.
- **New `NvimProcess::kill_in_place(&mut self)`** for the stall-restart path: set `shutting_down=true`, `child.start_kill()`, bounded `child.wait()`, `supervisor.abort()`.

### 3. stderr capture (nvim.rs:1112)
`.stderr(Stdio::null())` → `.stderr(Stdio::piped())`. Take `child.stderr`, `tokio::spawn` a `BufReader::lines()` drain task logging `tracing::warn!(target: "nvim_stderr", ...)` to EOF (prevents pipe-buffer deadlock).

### 4. Tier-1 open-file mirror

**init.lua — new `_G.wolf_setup_open_files(chan)`** (model on `wolf_setup_tabs`, init.lua:751): augroup `WolfOpenFiles` with `BufEnter`/`BufWinEnter`/`BufWritePost`/`BufFilePost`/`BufDelete`/`WinEnter`/`TabEnter`/`TabNewEntered`/`TabClosed`. `emit()` builds per-tabpage `{ tp_handle, active_path, files[] }` over `nvim_list_tabpages()`; per tab, iterate `tabpage_list_wins`, include `nvim_buf_get_name(buf)` only when non-empty AND NOT `b:wolf_preview_source`. `rpcnotify(chan, "wolf_open_files", list)`. Emit once immediately.

**render.rs — `Snapshot.open_files: HashMap<i64, TabFiles>`** where `pub struct TabFiles { pub active: Option<PathBuf>, pub files: Vec<PathBuf> }`. Out-of-band, wholesale-replaced, no `seq` bump (documented like `tab_cwds` render.rs:733-743). Default empty.

**redraw.rs:** `const OPEN_FILES_EVENT: &str = "wolf_open_files"`; match arm → `apply_open_files(args).await` then `(self.wake)()`; `async fn apply_open_files` mirrors `apply_tab_cwds` (build map + canonicalize with raw fallback BEFORE the write guard, then wholesale-assign). No `seq` bump.

**nvim.rs — `NvimProcess::setup_open_files`** mirrors `setup_tabs` (channel id → `nvim_exec_lua("wolf_setup_open_files(...)", [chan])`).

**api.rs `start`:** add `proc.setup_open_files().await?;` to the setup chain (restart re-runs identical chain).

**Front-end read-back (FFI):** `WolfCore::open_files_for_tabpage(handle) -> Option<TabFiles>`; opaque `FfiOpenFiles { active: String, files: Vec<String> }` with `active_path()` + `files()`; `WolfCoreHandle::open_files_for_tabpage(tab) -> FfiOpenFiles`. Front-end captures per view BEFORE restart (respawn wipes the mirror).

### 5. Restart entry point (wolf-core + wolf-ffi)

**Ownership prerequisites on `WolfCore`:**
- `nvim: NvimProcess` → `nvim: RefCell<NvimProcess>` (WolfCore is `!Send`, main-thread only). Every `self.nvim.<call>()` site becomes `let nvim = self.nvim.borrow(); self.block_on_rpc("…", nvim.<call>())` — bind the `Ref` to a local so no guard crosses `.await`.
- Input pump (api.rs:305-355) captures a fixed handle that goes stale after restart. Add `shared_handle: Arc<parking_lot::RwLock<Nvim>>`; pump reads `shared_handle.read().clone()` per batch. Restart swaps in the new handle.
- Store `handler: RedrawHandler` (it's `Clone`) so restart clones a fresh one carrying the SAME snapshot/intents/commands/wake.
- Store `nvim_cfg: NvimConfig` (reused verbatim — same XDG `state_root`, same durable `swap_dir`).
- Add `restarting: AtomicBool` (reentrancy guard).

**`WolfCore::restart_nvim(&self) -> Result<(), CoreError>`:**
1. CAS `restarting` false→true; if already true return `Ok(())`.
2. `runtime.block_on(self.nvim.borrow_mut().kill_in_place())` (works for crashed-or-wedged; old `shutting_down` set first so old supervisor won't re-fire Dead).
3. `liveness::reset_alive(&self.liveness, &self.wake)`.
4. Respawn on runtime: `spawn(&nvim_cfg, handler.clone(), liveness.clone(), wake.clone())` → `ui_attach(cols, rows)` → full `setup_*` chain + `setup_open_files`. Grid: reuse `nvim_cfg.initial_cols/rows` (next front-end resize re-establishes live size).
5. `*self.nvim.borrow_mut() = new_proc; *self.shared_handle.write() = self.nvim.borrow().handle();`
6. `restarting.store(false)`. Return `Ok(())`.

Restart does NOT reopen files/rebind — that's the front-end's job (§8) off the mirror captured pre-restart. Post-`Ok` contract: `current_tabpage()` = fresh tabpage-1, `list_buffers()` empty.

**FFI:** `WolfCoreHandle::restart(&self) -> bool` and `WolfCoreHandle::liveness(&self) -> u8` (returns `Dead` discriminant when core gone). FFI `liveness` discriminant module (ALIVE/NOT_RESPONDING/DEAD) mirroring `wolf_core::Liveness`.

### 6. Swap-file config (durable, stable per-project)

- **NvimConfig** — add `pub swap_dir: PathBuf` (NOT the ephemeral `TempDir`/`state_root`).
- **`state.rs::swap_dir_for(cwd: &Path) -> Option<PathBuf>`** = `state_dir().map(|d| d.join("swap").join(<stable-hash-of-canonical-cwd>))`. Deterministic hash (hex `DefaultHasher`) → SAME project reuses SAME dir across respawns AND separate Wolf processes; distinct projects distinct dirs. `WolfCore::start` computes from `cfg.cwd`, `create_dir_all`s, passes into `NvimConfig`. Unresolvable `state_dir()` → fall back to `state_root.join("swap")` (ephemeral, documented) — no panic.
- `NvimConfig::from_env` gains a `swap_dir` param; ephemeral `TempDir` unchanged (still hosts XDG isolation).
- `spawn` adds `.env("WOLF_SWAP_DIR", &cfg.swap_dir)`.
- **init.lua (replace `swapfile=false` at :25):** `opt.swapfile = true`; if `vim.env.WOLF_SWAP_DIR` non-empty, `opt.directory = swap_dir .. "//"` (trailing `//` = full-path-encoded swap names; drops the default leading `.`). Keep default `updatecount`/`updatetime`.
- Note: existing `tests/swap_exists.rs` plants `.<name>.swp` next to the file — no longer matches (directory now = `WOLF_SWAP_DIR`); superseded by new `swap_recovery.rs` (§9).

### 7. Recovery-aware `SwapExists` autocmd (init.lua:31)

Replace blanket `swapchoice="e"` with: read `vim.v.swapname`; `vim.fn.swapinfo(swapname)`; `local uv = vim.uv or vim.loop`. Determine `dead`:
- non-table / `info.error` → dead (stale/unreadable).
- `info.host` present and `~= vim.fn.hostname()` → NOT dead (another machine → uncertain → edit).
- numeric `info.pid > 0` → `uv.kill(pid, 0)` existence probe: `0` = ALIVE, `ESRCH`/failure = DEAD.
- else → dead (no owner pid).
If dead: `v:swapchoice="r"` + `vim.schedule(function() pcall(vim.fn.delete, swapname) end)` (post-recovery cleanup). Else `v:swapchoice="e"`. Never a blocking prompt.

### 8. Front-end (wolf-macos primary; wolf-gnome kept compiling)

- **`Project.onWake()` (Project.swift:140):** read `handle.liveness()` FIRST. `.dead` → fan `showNvimOverlay(.dead)` to ALL `views.allObjects` + auto `restartNeovim()` (guarded) + return (skip stale render). `.notResponding` → fan `showNvimOverlay(.notResponding)` + return (wait for button). `.alive` → fan `hideNvimOverlay()` then normal `currentView?.onRenderWake()`.
- **`NvimStatusOverlayView`** (subview of `PaneContainerView`, modeled on `EmptyStateView.swift`): opaque layer-backed, centered label + "Restart Neovim" `NSButton`. Messages: "Neovim crashed — restarting…" (Dead) / "Neovim stopped responding" (NotResponding). Button shown for NotResponding (→ `onRestart`); Dead auto-restarts (button optional fallback). `PaneContainerView.showNvimOverlay(_:)`/`hideNvimOverlay()` + `onRestart: (()->Void)?` wired to `project.restartNeovim()`.
- **`Project.restartNeovim()`:** guard `restarting`; CAPTURE Tier-1 files per view via `handle.open_files_for_tabpage(v.tabpageHandle)` BEFORE restart; `guard handle.restart()`; rebind each view — first view `bindFirstTabpage()`, rest `addTabpage(cwd:)` (Project.swift:87-96); `v.rebind(toTabpage:)`; `set_current_tabpage`; `handle.open(path)` for each file (active LAST so it lands focused); `setCurrent(currentView)`; `hideNvimOverlay()` all.
- **`WolfWindowController.rebind(toTabpage:)`** (new): updates `tabpageHandle` (WolfWindowController.swift:57) + `PaneContainerView.rebind(tabpageHandle:)`; next render wake repaints.
- **wolf-gnome:** new core APIs are ADDITIVE; `CoreConfig`/`start` unchanged → compiles with NO changes. Crash handling NOT wired into GTK this pass (README note + optional `// TODO(crash-recovery)`).

### 9. Test plan (real nvim, no mocks)

New integration tests under `crates/wolf-core/tests/`:
- **`nvim_liveness.rs` (case a):** `crash_marks_dead` — feed `:lua (vim.uv or vim.loop).kill(vim.fn.getpid(), 9)<CR>` (SIGKILL = crash), poll `core.liveness()` to `Dead`, assert wake fired. `stall_marks_not_responding` — short `WOLF_RPC_TIMEOUT_MS`, wedge via `:lua (vim.uv or vim.loop).sleep(60000)<CR>`, call a `block_on_rpc` method → `Timeout`, assert `liveness == NotResponding`; teardown exercises force-kill of wedged child.
- **`nvim_restart.rs` (case b):** open two temp `.md`, flush, assert both + active mirrored in `open_files_for_tabpage(current_tabpage())`. Self-SIGKILL, poll `Dead`, `restart_nvim().expect()`, assert `Alive` + fresh tabpage + empty `list_buffers()`. Drive reopen (`set_current_tabpage`, `Open` each), flush, assert `list_buffers()` has both.
- **`swap_recovery.rs` (cases c+d):** `WOLF_SWAP_DIR` → owned temp dir; control cwd. `dead_owner_recovers_silently` (c): open fixture, type unsaved edit, `:preserve<CR>` (deterministic swap flush), self-SIGKILL, `restart_nvim()` (same cwd ⇒ same swap dir), reopen, flush, assert recovered edit present AND `!snap.toast.is_prompt()`. `live_owner_edits_anyway` (d): core A opens+edits+`:preserve` (A alive); core B (same swap dir + cwd) opens SAME fixture, assert B shows CLEAN on-disk content (not A's edit) AND `!is_prompt()`.
- **Unit:** `liveness.rs` transitions/wakes/terminal-Dead/no-downgrade; `state.rs` `swap_dir_for` stable-for-cwd/distinct-for-distinct under `WOLF_STATE_DIR` override.
- **Swift:** overlay/button/rebind = manual-smoke (documented), like `EmptyStateView`.

### 10. Docs to update
`runtime/nvim/README.md`; `crates/wolf-core/README.md` + `src/README.md`; `crates/wolf-ffi/README.md` + `src/README.md`; `crates/wolf-macos/README.md` + `Sources/README.md`; `crates/wolf-gnome/src/README.md` (not-wired note); `crates/wolf-core/tests/README.md` (swap_exists superseded); root `README.md` + `architecture.md` (one-nvim-per-Project section, :65). `prettier --write` all edited Markdown; no hard line breaks mid-paragraph.

### 11. Risks / edge cases / open decisions
- Shutdown-vs-crash race: per-`NvimProcess` `shutting_down` set BEFORE qa!/kill.
- Restart reentrancy: `WolfCore.restarting` CAS + `Project.restarting` bool.
- Old vs new supervisor share one `liveness` Arc: old silenced via its own `shutting_down`.
- Swap-dir collision: per-project hash subdir; same project across processes intentionally shares (pid-aware `SwapExists` "e" for live owner handles it).
- SIGKILL/OOM loss window: edits since last swap sync lost; tests force `:preserve`. Documented.
- Unnamed buffers: out of scope (mirror skips empty names).
- **RESOLVED (orchestrator):** auto-clear `NotResponding` IS ADOPTED, with this factoring (confirmed after test-writer flagged the conflict): `reset_alive` = UNCONDITIONAL store-Alive, used ONLY by the restart exit path (restart's step 3 must move Dead→Alive). The RPC-success auto-clear is a SEPARATE guard AT the `block_on_rpc` call site: on a successful RPC, clear to Alive ONLY when the current state is `NotResponding` (compare_exchange NotResponding→Alive) — it must NEVER touch `Dead`. Do NOT route the RPC-success auto-clear through the unconditional `reset_alive`. A transient stall's overlay self-dismisses once nvim answers; Dead stays terminal until restart.
- **Open decision:** persist mirror to `state.json`? NO — live session state only; out of scope.

### Implementation order
1. `liveness.rs` + `pub use`. 2. `state.rs::swap_dir_for` + test. 3. nvim.rs (swap_dir, spawn sig, stderr, supervisor, fields, kill_in_place, shutdown). 4. render.rs (`open_files`/`TabFiles`). 5. redraw.rs (event/handler). 6. init.lua (swapfile/directory, SwapExists, wolf_setup_open_files). 7. nvim.rs `setup_open_files`. 8. api.rs (fields, start wiring, borrow migration, pump, timeout→NotResponding, restart_nvim, accessors). 9. wolf-ffi. 10. wolf-macos. 11. tests (+ replace swap_exists.rs). 12. docs + prettier.

## Status

- [x] Architect plan
- [x] Tests written (tests-first; expected NOT to compile until coder implements the API):
  - `tests/nvim_liveness.rs` (case a: crash_marks_dead, stall_marks_not_responding)
  - `tests/nvim_restart.rs` (case b: restart_respawns_and_reopens_open_files)
  - `tests/swap_recovery.rs` (cases c+d; SUPERSEDES swap_exists.rs — coder must remove swap_exists.rs + its target)
  - `src/liveness.rs` (doc + `#[cfg(test)] mod tests` — 15 unit tests; coder implements the enum+free-fns ABOVE the tests block, config.rs layout)
  - `tests/common/mod.rs` +`FlushWaiter::wait_for_wake(timeout)->bool`
  - `tests/state_unit.rs` + swap_dir_for stability test
  - `BUILD.bazel` + 3 new rust_test targets (nvim_liveness/nvim_restart/swap_recovery)
  - TARGET API the coder must match: `wolf_core::Liveness{Alive=0,NotResponding=1,Dead=2}` + `to_u8`/`from_u8(u8)->Liveness`; free fns `mark_dead`/`mark_not_responding`/`reset_alive`/`get` over `&AtomicU8`+`&WakeFn`; `WolfCore::liveness()->Liveness`, `restart_nvim()->Result<(),_>`, `open_files_for_tabpage(i64)->Option<TabFiles>` (`TabFiles{active:Option<PathBuf>,files:Vec<PathBuf>}`); `state::swap_dir_for(&Path)->Option<PathBuf>` (pub); `shutdown` force-kills a wedged child. Add `pub mod liveness; pub use liveness::Liveness;` to lib.rs.
  - Test-writer note on swap-dir lever: tests control it via WOLF_STATE_DIR + a fixed shared cwd (swap_dir derives from state_dir()/swap/hash(cwd)), NOT a test-set WOLF_SWAP_DIR. Coder: keep that derivation.
- [~] Implementation (coder pass 1 done — all 29 wolf-core + 31 wolf-ffi tests pass,
      clippy + swift-format green, macOS bundle builds). Deviation rulings below;
      one revert + 3 test changes pending before review.
  - Coder deviation rulings (orchestrator):
    - #1 removed `-n` launch flag: RATIFIED (necessary — `-n` sets updatecount=0,
      disabling swap; swap recovery impossible with it).
    - #2 `wolf_setup_open_files` uses global `getbufinfo({buflisted=1})` for files[]
      (active still per-tabpage): ACCEPTED PROVISIONALLY — flag for REVIEWER: getbufinfo
      is global to the nvim instance, so a multi-view/multi-tabpage project may reopen
      every file in every view, not just that view's. Reviewer must assess whether
      Tier-1 multi-view attribution is acceptable or needs per-tab tracking.
    - #3 `list_buffers()` filtering pristine unnamed buffers: REJECTED — unrequested
      user-visible buffer-picker change made only to satisfy a test. REVERT the filter;
      instead relax the `nvim_restart` post-restart assertion from `is_empty()` to
      "no file-backed buffers remain" (none of the previously-open named files present).
  - Pending test-writer pass (before review):
    - Relax nvim_restart post-restart assertion to "no file-backed buffers remain".
    - Add auto-clear test: wedge nvim with a FINITE short `vim.uv.sleep`, poll to
      NotResponding, then after the sleep ends call a block_on_rpc method and assert
      liveness()→Alive + a wake fired (covers the RESOLVED §11 auto-clear at the
      block_on_rpc call site).
    - Add restart reentrancy test: a nested/second `restart_nvim()` returns Ok(())
      without disturbing the live nvim (covers the `restarting` CAS guard).
  - Coder pass 2 DONE: reverted the pristine-unnamed `list_buffers` filter; new tests pass.
- [x] Implementation COMPLETE. Final: 29/29 wolf-core + 31/31 wolf-ffi tests pass, clippy
      (-Dwarnings) + swift-format green, `bazel build //crates/wolf-macos:Wolf` builds.
  - NOTABLE FIX (coder pass 2): the new recovery test exposed a REAL production bug —
    `timeout_block_on` used `tokio::time::timeout`, which DROPS the pending nvim-rs call
    future on elapse; that orphaned the response-channel receiver, so when a wedged nvim
    later recovered and delivered the response, nvim-rs's io loop errored ("Could not send
    response") and TERMINATED → supervisor spuriously reported Dead, permanently breaking
    the channel. Fix: `timeout_block_on` now SPAWNS the RPC as a runtime task and DROPS the
    JoinHandle on timeout (detaches it to run to completion), so the response reaches a live
    receiver and the io loop survives. Required converting 11 block_on_rpc-reachable
    NvimProcess methods (current_dir, buffer_path, list_buffers, render_preview,
    current_markdown_bufnr, open_preview, edit_source, preview_file, create_tabpage,
    current_tabpage, yank_visual_selection) from `async fn(&self)` to
    `fn(&self) -> impl Future + Send + 'static` (clone handle + own args into async move).
    setup_*/ui_attach/shutdown (direct block_on, not timeout path) unchanged.
  - Reentrancy: `restart_nvim` now early-returns Ok(()) when current liveness == Alive
    (no teardown of a live nvim); real flows only call restart on Dead/NotResponding.
  - Env flake noted: running all 62 tests + both lint targets in ONE combined invocation
    once killed `messages` mid-run (concurrent real-nvim load); passes cleanly in isolation
    + fresh full suite. Not a logic regression.
- [x] Review — APPROVED, NO BLOCKERS. timeout_block_on rework validated sound; liveness
      state machine, supervisor/shutdown race, restart_nvim, swap recovery all correct and
      faithful to plan + RESOLVED decisions. Findings routed below.
  - SHOULD-FIX #1 (ROUTING FOR CODE FIX): `restart_nvim` Alive-early-return returns
    Ok(())→FFI restart()==true→Swift unconditionally rebinds+reopens even on a no-op,
    producing DUPLICATE tabpages/buffers on a live nvim. Reachable via race (NotResponding
    button click racing nvim self-recovery to Alive). FIX CONTRACT:
    - `WolfCore::restart_nvim(&self) -> Result<bool, CoreError>`: Ok(true)=actually
      respawned; Ok(false)=no-op (current liveness==Alive, OR `restarting` CAS lost). On
      respawn/setup Err → `mark_dead(liveness)` (nice-to-have #4) + release guard + return Err.
    - FFI `WolfCoreHandle::restart(&self) -> bool`: Ok(true)→true, Ok(false)→false, Err→false
      (semantics now "an actual restart happened and succeeded").
    - Swift `Project.restartNeovim()`: `let didRestart = handle.restart()`; if didRestart →
      rebind each view to fresh tabpage + reopen files active-last + setCurrent + hide
      overlays (current behavior). ELSE → do NOT rebind; refresh overlay from a fresh
      `handle.liveness()` read (Alive→hide all; NotResponding→show notResponding; Dead→show
      dead), mirroring onWake's mapping. Ensure the `.dead` overlay ALSO shows the Restart
      button so a failed auto-restart is user-retryable (mark_dead wakes only on change, so
      there is no auto-retry loop — the button is the retry).
  - SHOULD-FIX #2 (→ technical-writer): document deviation #2 — the open-files mirror uses
    GLOBAL getbufinfo, so a multi-view/multi-tabpage restart reopens every project file in
    every view (active/focused file still correct). Record as a known Tier-1 limitation in
    runtime/nvim/README.md + wolf-core README/architecture.
  - SHOULD-FIX #3 (→ technical-writer): add the "crash recovery not wired into GTK this
    pass" note to crates/wolf-gnome/src/README.md.
  - NICE-TO-HAVE #4 (bundled into the #1 fix): mark Dead on respawn-failure path.
  - NICE-TO-HAVE #6 (→ technical-writer): document that a BrowserPreview server spawned
    before a restart keeps the old handle and re-renders blank until re-spawned (known limit).
  - NICE-TO-HAVE #5: detached-task retention across a permanent wedge — on record, no action.
  - Test impact of #1: `restart_nvim` return type Result<()>→Result<bool>. Two tests need
    updating: main restart (after Dead) now → Ok(true); redundant-on-live → Ok(false); fix
    the misleading "first, normal restart" comment (reviewer Q#7 — that first call is a
    no-op by design).
  - SHOULD-FIX #1 FIXED (coder): `restart_nvim -> Result<bool>` (Ok(true)=respawned,
    Ok(false)=no-op Alive/CAS-lost, Err→mark_dead + return Err); FFI restart() maps
    Ok(true)→true else false; Swift restartNeovim only rebinds when didRestart, else
    refreshes overlay from a fresh liveness() read; `.dead` overlay already shows the
    Restart button (retry). Tests updated + all green (29 wolf-core / 31 wolf-ffi / clippy
    / swift-format / macOS build). No re-review needed (localized, tests pin the contract).
- [x] Documentation — 11 Markdown files synced to the implementation (runtime/nvim, wolf-core
      README + src/README, wolf-ffi README + src/README, wolf-macos README + Sources/README,
      wolf-gnome src/README, wolf-core tests/README, root README, architecture.md). All three
      Should-fix/Nice-to-have doc items landed (getbufinfo Tier-1 multi-view limit; wolf-gnome
      not-wired note; BrowserPreview stale-handle limit) + SIGKILL loss-window + unnamed-buffer
      limits. `prettier --write` run by orchestrator on all 11 → all unchanged (already clean).

## MERGE FROM MAIN ✅ COMPLETE (commit 423a493)

Verified green on a CLEAN build: wolf-core 29/29, wolf-ffi 31/31, clippy (-Dwarnings),
swift-format, macOS bundle. origin/main is now a full ancestor of HEAD. Also fixed a
pre-existing main-side breakage in the macOS-only wolf-ffi `frame_pack.rs` test
(`pending_confirm` became pub(crate) but the cross-crate fixture used FRU syntax) by
constructing the Snapshot fixture field-by-field. Merge committed.

### (history) MERGE FROM MAIN — details

Merged origin/main (b5d2e99 ← abf6e2a: treesitter code-block syntax highlighting +
confirm-prompt/toast handling). 5 conflicts hand-resolved by orchestrator:
- `.claude/current-plan.md` → took ours.
- `crates/wolf-core/src/redraw.rs` → import union (`PendingConfirm` + our `TabFiles`).
- `crates/wolf-core/src/README.md` → union (our `liveness`/`Liveness` + main's markdown
  highlight re-exports).
- `crates/wolf-core/tests/README.md` → union (our liveness-unit clause + main's confirm-stash clause).
- `crates/wolf-core/src/nvim.rs` `render_preview` → kept OUR `impl Future + Send + 'static`
  signature (required by the timeout-spawn-detach) + main's highlighting doc/body-via-render_source_html.
Auto-merged high-risk files flagged for the coder to verify semantically: init.lua (both heavily
edited), render.rs, lib.rs, redraw.rs handlers. Merge STAGED, not yet committed — verification
pass (build + all tests + clippy + swift-format + macOS build) running before the merge commit.

## COMPLETE ✅ (pre-merge; NOT committed)

Feature delivered end-to-end: detection (supervisor on the io-loop + RPC-timeout escalation) →
three-state liveness (Alive/NotResponding/Dead) surfaced over FFI → per-Project restart
(`restart_nvim -> Result<bool>`) → Tier-1 open-file restore (wolf_open_files mirror) → swap-file
unsaved-edit recovery (durable per-project swap dir + recovery-aware SwapExists) → macOS crash/stall
overlay with a Restart button. NOTABLE production bug fixed en route: timeout_block_on now detaches
the RPC task on timeout so a recovering wedged nvim no longer kills the io loop. Final: 29/29
wolf-core + 31/31 wolf-ffi tests pass, clippy (-Dwarnings) + swift-format green, macOS bundle builds,
docs synced + prettier-clean. Reviewer APPROVED, no blockers. Known Tier-1 limits documented.
Manual smoke of the Swift overlay/restart/rebind flow recommended before shipping.
