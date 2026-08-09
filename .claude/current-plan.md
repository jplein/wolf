# Work unit 22 — the coordinator, and a second window

Branch: `linux-feat/multi-window` off `linux` (`5b754de`, clean and green:
980 tests / 94 suites, 58 cage smokes).

## Scope

**Row F — `Multiple windows`** (`feat-list.md:111`). Out of scope, deliberately:
native tabs (unit 23), the floating-panel picker (unit 24 — see O1), preview in
browser, the vendored plugins.

---

## Reconnaissance — measured on this host, take as fact

### M1. Baseline

`cargo test --workspace` exit 0, **980 passed / 0 failed / 1 ignored**, 91 test
binaries + 3 doc-test blocks. `smoke_headless` **58 passed in 474.22 s** — the
dominant cost. `wolf_core` lib 224, `wolf_gnome` lib 277. `cargo fmt --check` and
`prettier --check` both clean.

### M2. Two `WolfCore`s in one process are FULLY INDEPENDENT — the decisive measurement

Snapshots, wakes, tokio runtimes, swap dirs and loopback ports are all
per-instance. Driving A left B untouched and vice versa; `A.shutdown()` left B
`Alive` and holding its own frame. The only process-global state in `wolf-core` is
`canonical_home()`'s `OnceLock`, `render.rs`'s empty-`Grid` `OnceLock`, a test-only
`ENV_LOCK`, and `init_tracing`, which documents itself idempotent *"regardless of
how many projects/windows start"* and has its own test.

**Nothing in the core has to change for this unit.**

### M3. `crates/wolf-ffi/tests/multi_window.rs` ALREADY runs and passes on Linux

`multi_core_identity_snapshots_and_wake_routing ... ok`, 0.73 s, **no skip
banner**, two real hermetic Neovims with distinct wake tokens and no
cross-clobbering. **The Rust half of this row is already proven on this platform.**

### M4. Tabpages — background grids survive

`Snapshot.grids` does **not** drop a non-current tabpage's grids, so
`pane_view::detect_closes` would not prune a background view's panes. But
`Snapshot.windows` / `active_win` are current-tabpage-only. The `:tab*` lockout is
real and measured. (Relevant to unit 23, recorded here so it is not re-measured.)

### M5. Two windows on the SAME project collide on the durable swap dir

Core B opening a file core A had edited produced the toast
`W325: Ignoring swapfile from Nvim process <pid>`. It is a **warning, not a
blocking `confirm()`**, and the file opened. The shared state-dir MRU sqlite was
written and read correctly from both cores.

### M6. Two `StateStore`s over one directory SILENTLY CLOBBER — the real hazard

Both `flush()` returned `true`; the file held only store A's tree even though B
wrote last; each store still claimed its own tree in memory; a fresh open read back
A. **No panic, no corruption — one window's entire session simply lost**, and
non-deterministically. This is exactly what `wolf-ffi/src/lib.rs:1388-1396`'s
`SHARED_STORE` comment calls unsupported, and `wolf-gnome` opens its store inside
`build_ui` (`app.rs:273`, `:325`).

### M7. `cage` shows ONE toplevel at a time, and `present()` does not raise

Two `adw::ApplicationWindow`s in one process work under `cage` and
`app.windows() == 2`, but the newest toplevel stays on top and presenting the
backgrounded first window did **not** raise it. `cage(1)`: *"Cage runs a single,
maximized application… only a single one is visible at any point in time."*

`cage` does link `wlr_foreign_toplevel_handle_v1_*`, so a `wolf-pointer-inject`-style
client could enumerate and activate toplevels — **that tooling does not exist.**

### R2. `wolf-gnome`'s single-window assumptions — quantified

**Twelve structural sites**, of which the load-bearing ones are: `app.rs:304`
`build_ui` (~350 lines, one flat body per `activate`, with no window type to
instantiate twice); `app.rs:273/:325`'s per-activation `StateStore` (M6);
`persistence.rs:211` committing a **whole tree** containing only itself;
`state_seam.rs:206-234` hard-coding `windows: vec![one]`, whose own doc calls it
*"the ONE place that shape lives so a later unit has to change it deliberately"*;
`state_seam.rs:74-75` discarding all but the first saved window;
`command_action.rs:120` `NewWindow → Deferred`; `actions.rs:176-184`'s
`app.new-window` row; and `palette.rs:232` passing `&[]` where core already
supports `PaletteRowKind::SwitchWindow` and `PaletteWindow { index, cwd }`.

**Incidental but noteworthy**: `shell.rs:661-665` `app.`-scoped actions are added
once *per window*, last closure wins (benign today); `install_pane_css` /
`install_css` add a display-global provider on every `build()` while both doc
comments say "once".

**`wolf-gnome` contains ZERO `static` / `OnceLock` / `thread_local!` of its own**,
and everything in `ShellParts` is already window-scoped. **The shell is already
window-scoped; the missing piece is a coordinator above it, not a rewrite below.**

### R4. The state schema ALREADY supports the tree

`WolfState { windows: Vec<WindowState> }`, `WindowState { x,y,width,height, tabs }`,
`TabState { cwd, sidebar_visible, layout }`. **`STATE_VERSION = 1` needs no bump.**
Linux's Wayland caveat stands: only size restores, `x`/`y` are a deliberate `0.0`.

### R5. The action namespace already anticipates this

`actions.rs:108-110`: *"The `win.` prefix scopes the action to the window (so the
deferred multi-window row needs no rework)."* 22 of 24 rows are `win.`; only
`app.quit` and `app.new-window` are `app.`. `adw::Application` uses
`connect_activate` only — no `open`/`HANDLES_OPEN`/`NON_UNIQUE`.

### R6. The floating-panel picker row is NOT blocked behind this unit

`DECISIONS.md:95` and `feat-list.md:171` both say the GTK answer is **already
built** — the overlays are children of the window-level `gtk::Overlay`, and GTK 4
paints overlay children after the main child and its descendants. What is missing is
that *nothing captures a picker over a preview*. macOS needed a real child
`NSWindow`; **GTK does not.** The dependency is **one cage smoke**, not the
coordinator work.

### R7. A documentation/code disagreement to fix here

`wolf-core/src/state.rs:119-120` and `:173` say a tab *"is its own captive
Neovim"*. `wolf-macos/DECISIONS.md:42` and `Project.swift:3-14` say a tab is a
**tabpage of a shared** Neovim. `git log -S` dates both strings to `54a0a4d`, when
they were true; `919ab85` introduced the project model the same day and did not
update them. **The implementation is the fact; the comments are the drift.**

### P1–P3 (predictions, labelled)

**P1.** `cage_command` removes `DBUS_SESSION_BUS_ADDRESS`, so a second
`wolf-gnome` process under `cage` becomes its own primary instance rather than
sending `activate`. A smoke therefore **cannot** open a second window by launching
the binary twice; it must use a chord, the palette, or a command inside one process.
**P3.** `state_seam::may_snapshot`'s `reported_windows >= 1` gate would refuse to
snapshot from a background view, so a per-view contribution needs a different gate.

---

## Orchestrator decisions — binding, made before the plan

**O1. This is unit 22 of three, and the split is accepted.** `feat-list.md:238`
calls multi-window-and-tabs one item; the recon shows it is not. **Unit 22 —
the coordinator and a second window** (this one). **Unit 23 — tabs**, which needs
grid rebinding, per-view `capture_open_files`, per-view wake fan-out and an
`adw::TabView` this crate has never used. **Unit 24 — the floating-panel picker**,
which R6 shows is **independent of both** and needs one cage smoke.

The cut is chosen so this unit ships a falsifiable claim on its own: **two windows,
two Neovims, both sessions in one `state.json`, both restored at relaunch.**

**O2. Windows only. No tabs, no `adw::TabView`, no tabpage-per-view.** M2/M3 make
the multi-window half almost entirely GTK plumbing over a Rust layer already
proven. Tabs introduce failure modes nothing on Linux has executed. Mixing them
would put the unit's riskiest work behind its safest.

**O3. `StateStore` becomes process-wide; `persistence.rs` contributes a subtree and
the coordinator commits the tree.** M6 is not a theoretical race — it is measured
silent session loss. Follow macOS's shape (`wolf-ffi/src/lib.rs:1388`'s
`SHARED_STORE` plus a coordinator that walks every window). `state_seam.rs`'s own
doc invites this ("so a later unit has to change it deliberately"); do it there and
nowhere else. `STATE_VERSION` does **not** move (R4).

**O4. Restore is positional, not grouped by cwd.** macOS groups restored windows by
working directory because several *views* share a *project*; with one core per
window there is no grouping to do, and inventing one would merge two windows the
user deliberately opened. Restore `WindowState`s in saved order. If a saved tree
holds two windows on one cwd, restore both — and M5's `W325` toast is Neovim's own
honest warning, identical on macOS since the swap dir is core-side.

**O5. New Window opens a FRESH, EMPTY window with its own core** — macOS's
`makeNewWindow` (`AppDelegate.swift:421`) starts a fresh project with always-empty
state, and parity is this row's subject. Do **not** inherit the invoking window's
cwd (that is `makeNewTab`'s behaviour and belongs to unit 23).

**O6. The palette gains real `SwitchWindow` rows.** Core already supports
`PaletteRowKind::SwitchWindow` and `PaletteWindow { index, cwd }`; `palette.rs:232`
passes `&[]` only because there was never a second window. `open_recent_activation_targets_the_single_window`'s
own doc comment names the change: *"with a second window the right answer is to
raise it, not to hide the row."* Update that test deliberately — it is a guard that
exists to force this decision, not collateral.

**Raising is best-effort on Wayland and must be documented as such**: M7 measured
that `present()` does not raise under `cage`. On a real session xdg-activation
usually carries it; under the smoke harness it will not. Record the limitation
rather than asserting on it.

**O7. Do NOT scope the swap dir per core, and do NOT dedupe windows by cwd.** The
swap dir is deliberately durable across processes — that is what makes swap
recovery work — so per-core scoping would break a shipped feature to silence a
warning. M5's toast is correct behaviour.

**O8. The smoke asserts NON-VISUALLY. No `wlr-foreign-toplevel` helper.** M7 makes
a two-window capture impossible: `cage` shows one toplevel and `present()` does not
raise, so no capture can show two windows and none can bring the first back.
Commissioning a foreign-toplevel client is a `tools/` deliverable of its own and
would double this unit.

The evidence is therefore: **the count of `nvim` children of the app pid** (the
crash smoke already has `pid_is_live` and children-scan helpers), **`state.json`'s
two-window tree**, and **Neovim-side readbacks**. Where a capture is needed at all,
assert only on the newest window's pixels. **P1 is binding on the scenario**: the
second window must be opened by a chord, the palette or a command **inside one
process**, never by launching the binary twice.

**O9. R7's two stale comments are fixed in this unit.** They are in
`wolf-core/src/state.rs`, the file whose schema this unit starts using in full, and
CLAUDE.md makes a false comment a bug.

**O10. Hazards carried forward from units 20 and 21, binding on every agent.**
Never `git checkout --` / `git restore` / `git stash` — all work is uncommitted, and
a `checkout --` destroyed 1381 lines in unit 20. Never restore an `include_str!`'d
asset with `cp -p`; the preserved mtime makes Cargo reuse the cached build, so a
checksum reads OK while a falsifier is still compiled in. Plain `cp` + `touch`, and
re-run green before believing a restore. **No constant may be written before its
measurement exists, and no assertion ships without a captured red.** Any
pre-existing test that moves is a **finding to record, never a threshold to
retune**.

---

## The implementation plan

**The architect's full plan lives at
`/tmp/claude-1000/-home-jplein-dev-jplein-wolf/7a1ee4a2-ca6e-4c0e-89bf-439b63de901c/scratchpad/plan22.md`
— every agent must read it in full.** It carries the coordinator/window split
(§2.1), the state protocol (§2.2), window lifetime (§2.3), New Window (§2.4), the
palette rows (§2.5), the `app.`/`win.` install hazard (§2.6), the file-by-file
change list (§3–§4), the test plan T1–T20 with every falsifier (§5), the pass order
(§6), lint (§8), and what cannot be verified here (§9).

**The plan is approved in full** except where the decisions below override it.

## Orchestrator decisions on the plan — binding

**O11. Caveat 1 is ACCEPTED — O6 is satisfied by minting a `SwitchWindow` row, not
by un-hiding the Open Recent row.** My O6 wording quoted the old test's comment
("with a second window the right answer is to raise it") and read as though Open
Recent itself should become raise-or-open. The architect is right that it should
not: `open_recent_candidates`' refusal is scoped to **the invoking window's own**
cwd, and re-rooting *this* window onto *another* window's project is the wrong
action rather than a better one — it is O4's "do not merge two windows the user
deliberately opened", applied to the palette, and it would trip M5's `W325`. Core
already mints the raise row from the window list, so the capability arrives without
touching the seam. Two rows for one directory is the honest surface.

The test is still updated **deliberately**, as O6 requires: renamed, justification
rewritten, and a third half added naming the `SwitchWindow` alternative.

**O12. Caveat 2 is ACCEPTED. P3 does not bite in this unit.** With one core per
window, every window's Neovim is its own current tabpage, so `may_snapshot` is read
once per window against that window's own core and needs no change. The prediction
is correct but describes **unit 23**'s shape. Record it on the gate's doc comment so
unit 23 inherits the finding rather than re-deriving it.

**O13. Caveat 3 is ACCEPTED — correct the comments, do not hoist the CSS.** The code
is not wrong: two identical display-global providers are inert. The **comments** are
the false half, and `tests/README.md` records that neither the focus ring nor the
current-file bold reaches any pixel here, so the hoist would be an unverifiable edit
to a startup path. Fixing the false claim *is* addressing the divergence. Record the
hoist as a named follow-up with that reason.

**O14. Caveat 4 is FOLDED IN, not deferred.** `DECISIONS.md`'s "Crash and stall
recovery" says **nine** blocking RPCs are reachable from this crate while `app.rs`
and `shell.rs` both say **seven** and enumerate seven; `feat-list.md:230` also says
nine. The doc-agent is already rewriting sections of both files, so the marginal
cost is a grep. **Recount from source, never by arithmetic**, and correct whichever
sites are wrong — including `feat-list.md:230` if it is.

**O15. `feat-list.md:126`'s command counts are already stale before this unit**
(true figures 19/1/6, not the recorded 17/1/8) and this unit moves them again.
Correct to the **recounted** figures and **say in the pass report that the drift
pre-dated this unit** — do not let a correction of someone else's error read as
this unit's own bookkeeping.

**O16. The pass order is adopted as written** (§6: P1, G1, P2, G2, P3a, P3b, P3c,
G3, P4, G4a, P5, G4b, P6, P7, G5), with the orchestrator's standing role split:
verifier gates are run by a **coder**, and no coder writes an assertion. **G3 is the
regression gate where the behaviour lands** — baseline 980 / 94 / 58.

## Pass results

**P1 + G1 — coder-A — DONE AND GREEN.** `state_seam.rs` +228/−0, purely additive.
`980 passed / 0 failed / 1 ignored, 94 suites, 58 cage smokes` — **exactly the M1
baseline**, nothing moved, fmt and clippy clean, no `#[allow]` added.

The coder proved each of the four data-loss rules load-bearing **before** handing
over, using a throwaway probe outside the repo (wired into no target, deleted
after). Falsifying rule 1 launches the app with **no window at all**; rule 2 merges
both windows onto one project with **both layouts gone**; rule 3 yields
`Some(WolfState { windows: [] })`, the value that would overwrite a good save with
an empty file. Each restored and checksum-verified.

**It also named its own gap plainly**: nothing outside `state_seam.rs` references
any of the new items, so **P1 ships production code with zero coverage by design**
and G1's 980 green says nothing about it. That is P2's whole job.

**M8. `MAX_RESTORE_WINDOWS = 16` is policy, chosen by the coder** — the plan fixed
it as policy but named no value. It bounds the pathological case at 16 Neovim
processes. **T5 must read the symbol, never a literal**, so moving it retunes
nothing.

**O17. P1's four test recommendations are ACCEPTED into P2**, each closing a way a
test could pass for the wrong reason:

- **T3's fixture must accept the process cwd AND both saved dirs.** If it omits the
  saved dirs, index 1 falls to `EmptyState` and **T3 passes for the wrong reason**.
- **A companion to T3** — `every_window_past_the_first_keeps_its_own_project`:
  `restore_plans(Some(&two_window_tree), None, home, ..)` yields both saved cwds
  with **both layouts attached**. Without it, T3 is satisfied by a `restore_plans`
  that simply drops every layout past index 0 — a different bug with the same
  symptom.
- **T7 must include `(true, 0)`**, not only `(true, ≥1)`. Arm-reordering showed
  that is the row catching a match written with `(false, 0)` first.
- **T6b needs no `float_cmp` allow** — comparing `Vec<WindowState>` uses the derived
  `PartialEq`. Only T6a's direct `window.x == 0.0` needs the existing allow, moved
  verbatim.

**Note for P3a**: `plan_for_window` and `restore_plan` currently hold identical
bodies. That duplication is transitional and resolves when P3a deletes
`restore_plan`; **if P3a keeps both, they can drift silently.**

§2.2's code block contradicts its own prose on `snapshot_state` vs
`snapshot_state_tree`; the prose (and §4's table) win, and P1 followed them.

**P2 + G2 — DONE AND GREEN. 992 / 94 / 58** (980 + 12, all in `wolf-gnome --lib`
277 → 289). All twelve falsifiers captured red, each alone, all restores
checksum-verified. **No test resisted falsification.** fmt and clippy clean.

P2's five isolation findings **all confirmed by measurement**, correcting §5's table
in four places. The sharpest: **§5's T1 falsifier as literally written reddens
nothing** — every row's process cwd is either `$HOME` or the saved cwd itself, and
the one row that would move reaches its plan through the floor. The blanket edit is
what reddens it, and **T3 is the only guard on where the process cwd is offered**,
so P3a must not weaken it. §5 also names the wrong companion for T1's
non-isolation, and calls T2 isolated when its revert reddens four tests.

**O17's third item is now measured**: under arm reordering, `(true, 0)` is the row
that fires **and the only one**. Without the row O17 added, that bug ships silently.

**O18. Two G2 carry-forwards bind on P3a and G3.**

- **G3 must re-run T7-i and confirm the two relaunch smokes then DO redden.** With
  `close_commit` still uncalled, G2 measured them **green** under that falsifier
  (18.26 s, 2 passed). Once P3a wires the close path they must redden — that is the
  cheapest available proof the coordinator genuinely routes through `close_commit`,
  and it costs 18 s.
- **After P3a, re-run T1-blanket and T6a-i/ii and expect them to redden
  `a_layout_is_never_restored_into_a_different_project` and
  `snapshot_state_is_one_window_one_tab`.** G2 measured both **isolated** today only
  because P1 duplicated the bodies. **If they do not redden, the deletion did not
  happen.**

**P3a — coder-B — DONE AND GREEN.** `coordinator.rs` (470 lines) and `window.rs`
(862) added; `app.rs` +57/−735; `persistence.rs`, `shell.rs`, `lib.rs`,
`state_seam.rs` reworked. **991 passed / 94 suites / 58 cage smokes**, fmt and
clippy clean, no `#[allow]` added. Delta **−1**, accounted exactly to
`restore_plan_table`'s mandated deletion. Nothing moved down, as R2 predicted; the
coder listed five things it noticed and **deliberately did not tidy**.

**Four tests had to be repointed, not the brief's two** — the brief's list was one
short of what the deletion forced. All assertions byte-identical.

### M9. O18's first carry-forward FAILED — and the investigation is the finding

Under T7-i the two relaunch smokes **stayed green**. So did the stronger
`(false, 0) → Skip`. The coder did not move on; it diagnosed and then built a
falsifier that discriminates.

**Why they cannot see it**: the per-wake `commit` has already written an identical
tree by the time `:qa` reaches the drain, so the close path's commit is redundant on
these scenarios and its flush is belt-and-braces. **This is pre-existing, not a
regression** — the old `flush_on_close` was the same redundant call, and G2 measured
the same green while `close_commit` had no caller at all.

**The routing IS proven**, by a two-step falsifier: disable the per-wake commit
alone → both smokes stay **green** (so the close path saved the session on its own);
add T7-i on top → both **redden**, one with `exit 16: run 1 wrote NO state.json at
all`, the other restoring `Untitled.md` instead of the three-pane layout.
**`close_commit`'s answer is what decides whether a session survives a close.**

### M10. §5's T7 prediction CONTRADICTS §2.2's rule 3 — rule 3 wins

§5 predicted `DropSubtree` on the last window yields `windows: []`. It cannot: rule
3 turns an empty contribution set into "commit nothing". So on the last window the
arm is **indistinguishable from a no-op**. `tests/README.md` and `DECISIONS.md` must
record the `DropSubtree` arm as unproven **for that second reason**, not merely "no
smoke closes one of two windows".

**O19. §2.6's fix is already covered by T20 step 6 — no extra smoke.** P3a found
that **nothing in the suite exercises `app.quit` at all**: every existing `:q`/`:qa`
smoke goes through `win.close-window`, so the `app.`/`win.` install split and
`Coordinator::quit`'s ordering are uncovered. But T20's step 6 **is**
`Ctrl+Shift+Q`. P5's test-writer must know that step 6 is §2.6's **only** coverage
and capture its falsifier: delete `Coordinator::install_app_actions`' body → the
chord falls through to Neovim as `<C-Q>`, the app stays alive, and the existing
exit-12 guard fires.

**O20. Three smaller P3a findings, assigned.**

- **`mint_id`'s live-id skip is unreachable by any test** (it matters after 65 536
  windows). Extract the arithmetic as a pure `next_free_id(next, live)` so P4 can
  table-test it in three rows — cheap, and it removes untestable code rather than
  documenting around it.
- **Record the close path as unproven** in `tests/README.md`, with M9's measured
  reason: the close-path commit and flush are redundant with the per-wake commit on
  every existing scenario, and no smoke discriminates them.
- **`close_commit_table`'s doc is stale** — it says the reds are confined to the
  seam "while `close_commit` still has no caller". M9 is the replacement.

**O21. Prose P3a falsified but does not own — added to P6.**
`workspace.rs:14`'s dangling `crate::state_seam::restore_plan` link (the crate's
only unresolved intra-doc link, and **nobody in §6's pass table owns
`workspace.rs`**); `pane_view.rs:3211`, `tree_view.rs:839`, `palette.rs:428`
referencing the gone `app::build_ui` / `app::wire_close`; `paint_golden.rs:144`'s
`app::measure_metrics`, now `pub(crate)`. **P5 owns** `smoke_headless.rs`'s exit-code
strings naming `flush_on_close` and `app::TREE_PANE_WIDTH`.

**P3a's five deviations from §2.1–§2.6 are all ACCEPTED**: `WindowInner` without
`shell` (an unread field would need `#[allow(dead_code)]`); `quit` taking
`&gtk::Application` (what `GtkWindowExt::application()` returns);
`coordinator::build()` not `new()` (`clippy::new_without_default` is default-on
under `-D warnings`); and `pub` not `pub(crate)` (the combination makes every `pub
fn` returning it a `private_interfaces` error).

**P3b + P3c — DONE.** New Window classifies and dispatches; the `app.new-window`
row is menu-listed; `PaletteAction::SwitchWindow`, the `WindowSource`,
`palette_windows`, `raise` and `display_cwd` all landed. `next_free_id` extracted
per O20. fmt and clippy clean, no `#[allow]`. **990 passed / 1 failed**, 94 suites,
58 cage smokes at the baseline cost.

**By-hand `cage` evidence** — currently the only proof these two passes work.
New Window from the palette took `nvim` children **1 → 2 in one process**; window A
read the project cwd and focused window B read `$HOME`, so **O5's no-inheritance
(F3's discriminator) is measured**; the frozen `state.json` held a **two-window tree
in order**. The `switch` query listed exactly one row — window A under its persisted
cwd, self excluded — and activating it left both cores alive, with `present()` **not
raising**, precisely M7.

### M11. One pre-existing test is RED, and its premise is unrecoverable

`palette_rows_query_new_selects_exactly_the_new_buffer_row` returns 2 rows for
`"new"` — `["New", "New Window"]`. **O10 was honoured: it was not retuned.** The
test is doing exactly what its own doc says it exists for, having predicted that "a
second `new`-matching command being promoted to dispatched … would make the smoke
stochastic".

Measured, and it changes the fix: **no query can restore the "exactly one row"
premise.** `score_entry` requires every term to be a case-insensitive *subsequence*
of `display`, and **"New" is a prefix of "New Window"**, so every query matching one
matches the other. The guard must become an **order** guard.

The ordering is deterministic — `rank_matches` sorts on `(mru_group, mru_rank,
Reverse(score), match_start, span, len, display)`; "New" and "New Window" tie
through `span`, and `len` (3 vs 10) decides. The cage smoke it guards **still
passes** (7.49 s, alone and in the 58/58 block).

**The MRU premise becomes load-bearing**: with `"new_window"` in the command MRU,
`mru_group` would put New Window first and the smoke would activate the wrong row.
The smoke's per-run `WOLF_STATE_DIR` keeps that store empty — **an assumption now
worth asserting rather than one that happens to hold.**

**O22. P3b/P3c's three plan contradictions are ACCEPTED.** §2.4's `perform` arm
does not compile as written (the shell holds a `gtk::Application`, `open_new_window`
took `&adw::Application`, and `app.rs` is out of scope) — resolved by
`&impl IsA<gtk::Application>`, touching no excluded file. §4's `display_cwd()`
placement is unreachable from P3c's file set, so `WindowHandle::display_cwd` reads
`cached_subtree()` — **the same string the window persists, by identity rather than
by a second application of the rule**, which is a stronger join than recomputing.
And §2.4/§5 missed the palette-query collision entirely (M11).

**O23. P3c ships with zero automated coverage today**, exactly as P1 did — every
`SwitchWindow` behaviour is executed by no test. P4 closes it.

**O24. Two more stale-prose sites for P6** (O21's list): `nav_ui.rs:17` still calls
`snapshot_state` "the one documented single-window/single-tab site" — the
single-window half is false since P3a; and `refresh_coalescer.rs:161` says "a
single-window front-end", where the substance holds but the phrase does not.

**P4 + G4a — DONE AND GREEN. 997 / 94 / 58**, `wolf-gnome --lib` 288 → 294, zero
delta to account, no pre-existing test moved. **Every falsifier measured; no test
resisted falsification.** M11's red is fixed as an **order guard** with the MRU
premise asserted, and both its falsifiers land.

### M12. A test had been SILENTLY DISARMED, and the repair is confirmed

`palette_rows_cap_counts_only_actionable_rows` depended on `COMMAND_SPECS[0]` being
undispatched. P3b's promotion of `NewWindow` broke that premise, so `max == 1`
returned one row under **both** the correct and the reverted implementation — the
test stayed green while proving nothing. P4 replaced the hardcoded cap with one
**computed** from the position of the first `Unsupported` candidate. G4a confirmed
the repair re-arms it: under the falsifier it reports 3 rows where 4 are due, and
measured that the old assertion would indeed have stayed green.

**This is the second time this unit found a guard disarmed by a change elsewhere.**

**O25. Three G4a recommendations — two accepted into P5, one declined.**

- **ACCEPT: `palette_rows_offer_one_switch_row_per_other_window` needs a 2+ window
  case.** It catches a `.first()`-shaped bug the one-window case cannot. **P4 and
  G4a disagree on whether it already exists** — P4's report claims a two-window
  case, G4a read the file and says none. **P5 must check and add it if absent**,
  and say which was right.
- **ACCEPT: `next_free_id_table` needs a non-contiguous row** (`next: 5,
  live: [6, 7]` → `5`). All six present rows would pass an implementation scanning
  `live` for any member ≥ `next` rather than for `id` itself.
- **DECLINE: a table over `Coordinator::perform`'s accepted `UiAction` set.** The
  hole is real (if `APP_SCOPED_UI_ACTIONS` and `perform` drift the same way, both
  tests stay green) but it is a drift-in-lockstep of two hand-maintained lists,
  which a third hand-maintained list does not close. Record as unproven.

**`NEW_WINDOW_PALETTE_QUERY` is `"new window"`** — lowercase, one ASCII space,
verified with `cat -A` at `palette.rs:752`, in the **test** module following this
file's convention for its four other smoke queries. **No `pub const` exists**;
§4 assigned one to P3c and it never landed. **P5 must carry `"new window"` verbatim
into `smoke_headless.rs`**, joined by
`the_new_window_palette_query_selects_the_new_window_row`.

## Status

Decisions O1–O25 binding. P1–P4, G4a done. **P5 next — the smoke.**
