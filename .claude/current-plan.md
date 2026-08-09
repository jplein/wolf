# Work unit 21 — the GTK terminal drawer

Branch: `linux-feat/terminal-drawer` off `linux` (`2467624`, clean and green:
969 tests / 94 suites, 55 cage smokes).

## Scope

Two `feat-list.md` rows in the **Terminal** table, both unticked for Linux:

- **Row D** — `Terminal drawer (⌃\`)` (`feat-list.md:202`)
- **Row E** — `Terminal defaults to full width` (`feat-list.md:203`)

Out of scope, do not touch: multi-window, tabs, preview-in-browser, vendored
`orlando` / `lighthouse`, the application icon.

---

## Reconnaissance — measured on this host, take as fact

Every item below is either measured on this host (M), read from code (R),
a documentation/code disagreement (D), or a labelled prediction (P). Cite these;
do not re-derive.

### M1. Baseline

`nix develop --command cargo test --workspace` exit 0 — **969 passed, 0 failed,
1 ignored**, 94 suites; `smoke_headless` **55 passed in 440.85 s**, the dominant
cost. `cargo fmt --all --check` and `prettier --check "**/*.md"` both clean.

### M4/M5. The drawer already has live Linux coverage — the biggest surprise

**`crates/wolf-ffi/tests/terminal.rs` already runs and passes on Linux under
`cargo test`** — `terminal_drawer_defaults_to_full_width` and
`terminal_toggle_persists_and_recreates`, 2 passed in 0.41 s, **no SKIP banner**,
driving a real Neovim. `crates/wolf-core/tests/nav_history_live.rs:655` also
drives `core.toggle_terminal()` against a real nvim.

**So the suite already spawns real shell jobs today, and the Neovim half of both
rows is already proven on Linux.** The genuinely new surface is GTK-side only.

### M6–M8. What Neovim does, measured through the production `WolfCore` API

Three runs, 100x40 attach, pinned nvim 0.12.2.

- Attach: `grids = {1: 100x40, 2: 100x40}`, one window `win=1000 → grid 2`.
- After `toggle_terminal()`: exactly one intent, `DrawerOpen { win: 1001 }`.
  **The drawer's grid id is 4, not 3**, in all three runs; polled at 10 ms, no
  grid 3 was ever observed. **There is no transient grid to race.**
- `win_pos`: editor `grid 2 → (row 0, col 0, 100x26)`, drawer
  `grid 4 → (row 27, col 0, 100x13)`. `13 == floor(40/3)`, matching
  `runtime/nvim/init.lua:1231`. **Row 26 of grid 1 is nvim's own statusline
  separator** and carries the editor filename — grid 1 is not empty once a
  horizontal split exists, and `wolf-gnome` paints no grid 1.
- `win_buffers[1001] = { bufnr: 2, path: Some("term:///…/bin/bash"),
  is_terminal: true, is_drawer: true }`; `full_width_for_win(1001) == true`.
- `cursor.grid == 4`; `active_win == Some(1001)`.
- **The drawer takes input through the normal key path with no special casing** —
  `echo WOLFPROBE` typed via ordinary `InputEvent::Key` appeared on grid 4.
- `core.pane_status(4)` → `filename = "bash"`,
  `display_path = "term:///…/bin/bash"`.
- Second toggle (hide): grid 4 gone, grid 2 back to 100x40, and
  **`drain_intents()` returns `[]`** — no close intent, exactly as
  `crates/wolf-core/src/layout.rs:22-24` documents.

### M9/M10. Sizing — the drawer MUST be driven from its own allocation

`resize_grid(2, 100, 40)` with the drawer open left grid 2 at 100x40 and the
drawer untouched at 13x100, while `win_pos` still reported `h=26` for grid 2 —
so **per-grid sizing is authoritative and independent, and `WinInfo.height` goes
stale the moment the front-end drives its own sizes.**

Outer `resize(120, 50)` re-divided **only** grids the front-end had not
explicitly sized: grid 1 → 120x50, drawer → 120x23, editor grid 2 **unchanged at
100x40**. **A GTK drawer must therefore issue its own `resize_grid` from its own
allocation, exactly like every other pane.**

### M11. Without a focus seam the user is TRAPPED — this is the crux of Row D

`<C-w>k` from inside the drawer emits `FocusDir { dir: Up, from: 1001 }` and
**nvim does not move focus** (`active_win` stayed `Some(1001)`) — the pure-intent
asymmetry. In `wolf-gnome` today `apply_focus` (`pane_view.rs:984`) finds no pane
for win 1001, falls back to `active_pane` (the editor), and
`neighbor(editor, Up)` is `None`, so **nothing happens**.

### M12/M13. Process lifetime is safe

`:qa` with a live shell job raises no `E947`, no prompt, no toast:
`drain_commands()` returned `[CloseWindow]` and `liveness()` stayed `Alive`. The
bash child's ppid is the embedded nvim, and after `core.shutdown()` there were
**0 `bash` rows** in `ps` machine-wide. **No leaked process after any probe run.**

### M15–M17. What the GTK app does today, measured under `cage`

Typing `:lua _G.wolf_toggle_terminal()` into the running app opened the drawer and
moved Neovim's focus into it (`wins=2 cur=1001 termwins=1`). The two `grim`
captures are **visually identical apart from the cursor**: the editor still fills
the pane area, **no terminal strip anywhere**, and the block cursor is *absent*
in the second capture because `draw_pane` gates it on `snap.cursor.grid == g`
(`pane_view.rs:2064-2068`).

**Direct confirmation of `feat-list.md:205`: a `:terminal` window GTK never
paints, with keystrokes going into an invisible shell.** No crash, no warning.

### R-chain. The end-to-end chain, cited

| Link | Location |
|---|---|
| Lua helper | `runtime/nvim/init.lua:1195` `_G.wolf_toggle_terminal()`; height `math.max(1, floor(lines/3))` `:1231`; `botright <h>split` `:1234`; `b:wolf_full_width = true` `:1248-1249`; `wolf_emit_drawer(w)` `:1255-1258`; `startinsert` `:1271` |
| Intent emitter | `init.lua:935-940` (`action = "drawer_open"`) |
| Terminal-mode chords | `init.lua:976-979` (`t`-mode `<C-w>h/j/k/l`), re-entry autocmd `:986-994` |
| Input event | `crates/wolf-core/src/input.rs:275-278`, dispatch `:470`, `exec_lua` `:596-604` |
| Core API | `crates/wolf-core/src/api.rs:1034` `toggle_terminal` |
| Intent type | `crates/wolf-core/src/layout.rs:75-84`; the asymmetry rationale `:18-24` |
| Snapshot flags | `crates/wolf-core/src/render.rs:518-528` `is_terminal` / `is_drawer` |
| **GTK drop** | `crates/wolf-gnome/src/pane_view.rs:946` — `LayoutIntent::DrawerOpen { .. } => {}` |
| **GTK deferral** | `crates/wolf-gnome/src/command_action.rs:150-153` |
| **Reserved chord** | `crates/wolf-gnome/src/actions.rs:276-283` — `win.terminal`, `<Control>grave`, `BindingStatus::Reserved` |

### R1. Two independent switches gate the feature

`install_actions` (`shell.rs:614-673`) skips any row whose `ui_action_for` is
`Deferred`, and `action_for_key` (`actions.rs:481-495`) only scans `Bound` rows.
**Both `command_action::ui_action_for(Command::Terminal)` and `ACTION_TABLE`'s
`status` must change**, or the chord is inert.

### R3. The macOS design, for parity judgement

- `DrawerLayout.swift:29-66` — a **pure `Double` seam**. `defaultWeight = 1/3`;
  `drawerHeight = hasDrawer ? round(paneAreaHeight * weight) : 0`;
  `editorHeight = max(0, paneAreaHeight - drawerHeight)`. **No divider drag.**
- `PaneContainerView.swift:228` — `drawerPaneId` is held **outside** `tree`,
  explicitly *"so the tree keeps its 1:1 parity with wolf-gnome"*. `paneRects()`
  tiles the tree over `editorHeight` and **appends** the drawer rect.
- `applyDrawerOpen` `:2097-2104` — idempotent; binds the pane, `setActivePane`.
- `applyFocus` `:2118-2145` — from the drawer **only `.up` acts**, resolved by
  `DrawerFocusSeam.drawerUpTarget` (MRU editor leaf, else bottom-most leaf); from
  an editor pane a `nil` neighbour going `.down` **bridges into** the drawer.
- `detectCloses` `:2157-2176` — the drawer is removed **without** the last-leaf
  guard and **without** `tree.remove`.
- `setActivePane` `:2799-2806` records `lastEditorPane`, **never** the drawer.
- `layoutSnapshot()` `:1960-1966` — *"The drawer is excluded — it lives outside
  `tree`, so it never appears in `tree.layout()`."*
- `applyLayoutRebuild` clears `drawerPaneId = nil` `:2027`.
- Both Swift seams are unit-tested and wired: `BUILD.bazel:269-270`.

### R4. Row E is entirely an `init.lua` change, already shipped

`0506e92` is 18 added lines: the `b:wolf_full_width = true` seed plus two guarded
re-emits. **There is no front-end special-casing on macOS** — the flag rides the
ordinary `full_width_for_win` path, which `wolf-gnome` already consumes.

### R5. `DRAWER_OPEN` is not the only route

`Snapshot.win_buffers[win].is_drawer` identifies the drawer from a plain snapshot
read, so a GTK implementation has a **level-triggered** source as well as the
edge-triggered intent. Neither front-end uses it that way today.

### R6. The painter needs NO change

`draw_pane` (`pane_view.rs:1962`) resolves `snap.grid(g)` and paints generically.
**Nothing in `pane_view` or `render` branches on buffer type**; the only per-pane
branch is `is_preview` (`:2053`). **The gap is purely layout intent + pane
binding + geometry, not painting.**

### R8. The smoke harness can host a terminal smoke

`require_smoke_env` (`smoke_headless.rs:2326`) demands five tools on `PATH`,
`WOLF_NVIM_BIN`, `WOLF_RG_BIN` and a writable `XDG_RUNTIME_DIR`; it **panics
rather than skipping**. `cage_command` (`:10040`) **inherits `SHELL`** — measured
to be the devShell's pinned bash — so `:terminal` has a usable shell under cage.
Every smoke holds a shared mutex, and a hung one holds the Wayland socket with no
timeout.

### D1/D2. Two documentation defects, both falsified by measurement

- **D1.** `pane_view.rs:929` points at `crates/wolf-gnome/src/README.md` for the
  drawer gap. That file contains **no occurrence of "terminal" or "drawer"**. The
  gap is actually recorded at `crates/wolf-gnome/DECISIONS.md:258`.
- **D2.** `pane_view.rs:924` claims wolf-gnome *"exposes no way to reach it … so
  this arm is unreachable in practice."* **M15/M16 falsified this**: `:lua` is not
  abbreviation-blocked, so typing `:lua _G.wolf_toggle_terminal()` reaches the arm
  in the running app. True of the *bound surfaces*; false of the arm.

### P2/P3. Two predictions the plan must design against

- **P2.** If the drawer becomes a `PaneTree` leaf, `layout_snapshot`
  (`pane_view.rs:867`) will persist `abs_path = "term://…"` into `state.json` and
  `wolf_rebuild_layout` will `:edit` that string at next launch.
- **P3.** Binding the row will break three `#[cfg(test)]` tables: `actions.rs`'s
  `EXPECTED_BOUND_ACTIONS: [&str; 12]` (`:616`), `command_action.rs`'s
  `EXPECTED_DISPATCHED_IDS: [&str; 18]` (`:202`), and its `CASES` row at `:355`.
  `unimplemented_reasons_are_distinct_and_non_empty` also sees one fewer reason.

---

## Orchestrator decisions — binding, made before the plan

**O1. The drawer lives OUTSIDE `PaneTree`, matching macOS.** This is not a
preference. `PaneContainerView.swift:225` says macOS put it outside *"so the tree
keeps its 1:1 parity with wolf-gnome"* — the macOS choice was made **for** this
front-end, and declining it would strand that comment. Putting it inside the tree
is less code and breaks three things at once: P2's `term://` path persisted into
`state.json` and `:edit`ed at relaunch, `set_divider_fraction`, and every
`neighbor` result.

The cost is accepted: a Linux analogue of `DrawerLayout` as a **pure seam**, plus
the drawer becoming a fourth structural element in `rebuild_pane_area` (the
existing nested `gtk::Paned` tree wrapped in a vertical container). Keep the seam
pure and unit-tested in the fast tier, as macOS does.

**O2. `layout_snapshot` must exclude the drawer, and that exclusion needs its own
test.** P2 is the failure this unit must not ship. macOS states the rule
explicitly at `PaneContainerView.swift:1960-1966`; the Linux analogue gets an
assertion, not a comment.

**O3. The focus seam ships with this unit. Shipping the drawer without it would
ship a trap.** M11 measured that a user who opens the drawer cannot leave it:
`<C-w>k` is a pure intent nvim declines to act on, `apply_focus` finds no pane for
win 1001, and the fallback neighbour is `None`. Port macOS's `DrawerFocusSeam`
(MRU editor leaf, else bottom-most leaf) **and** the editor→`.down`→drawer bridge.
Both are pure seams and both are unit-tested on macOS; mirror that.

**O4. Row E costs no front-end work and must be verified, not implemented.** R4
measures it as already shipped in `init.lua`, riding the ordinary
`full_width_for_win` path that `wolf-gnome` already consumes, and M7 measured
`full_width_for_win(1001) == true` on this host. The row's tick therefore rests on
**observing full width in the running GTK application**, not on new code. If it
needs code, that is a finding — report it rather than quietly widening Row E.

**O5. The drawer's grid is sized from its own GTK allocation.** M9/M10 make this
mandatory, not stylistic: nvim re-divides only grids the front-end has not sized,
so a drawer left unsized drifts the moment the window resizes. It goes through
the same `resize_grid` path every other pane uses.

**O6. `<Control>grave` is NOT to be bound until its safety is measured.** The
action row's own note calls the chord's freedom *"an assumption and not a verified
fact"*, and the accelerator namespace's invariant is that no Neovim key becomes
unreachable. Three things must be measured first: what `<C-\`>` means to Neovim in
every mode including terminal mode; what byte GDK actually delivers and what the
translation seam makes of it; and **whether a GTK accelerator wins over the key
handler when focus is in the drawer** — which decides whether the chord can close
the drawer it opened. If it cannot, the chord is a one-way door and O3's focus
seam becomes the only exit. A measurement is in flight.

**O7. D1 and D2 are pre-existing doc bugs and are fixed by this unit.** Both are
in `pane_view.rs`, both concern the arm this unit replaces, and CLAUDE.md makes a
false comment a bug. D2 in particular is falsified by this unit's own measurement.

**O8. P3's three `#[cfg(test)]` tables are expected to break and their updates are
part of the change, not collateral.** They are guards that exist to force exactly
this classification decision. Any agent that edits one must say which and why.

---

## Second reconnaissance — chord safety and the restart hole. MEASURED.

### M18. `<Control>grave` is SAFE and REACHABLE. Bind it.

- **Nothing maps it.** `maparg('<C-\`>', m)` is `''` for every one of `n`, `i`,
  `v`, `t`, `c`. `:silent! helpgrep CTRL-\`` → **0 quickfix entries** (control:
  `CTRL-@` → 12). `actions.rs:281-282`'s "assumed free" is now **verified**.
- **But it is not inert.** `nvim_input` folds Ctrl+backtick to **NUL**, because
  `0x60 & 0x1F == 0x00`. Measured three ways: in insert mode it raises
  **`E29: No inserted text yet`** and drops out of insert (`i_CTRL-@`); `<C-@>`
  through the identical sequence gives byte-identical results; and in terminal
  mode it **is forwarded**, with `cat -v` echoing `X^@Y`.
- **GDK delivers no terminal folklore.** keyval `grave`, raw `0x60`;
  `gdk_key(grave, CONTROL)` → `Char('\`') + ctrl`; `key_to_notation` →
  `"<C-\`>"`. **No `0x1e`, no `^^`, no drop.**
- **M18a — a bound chord WINS in terminal mode.** With the drawer open and
  Neovim in terminal-insert, `wtype -M ctrl -M shift -k b` fired
  `win.toggle-sidebar`: filler rows went 6 → 0. **The chord is not a one-way
  door.** Mechanism: `shell.rs:611-612` is a **capture-phase** controller on the
  *window*, and GTK focus never lands on a pane at all, so it is ahead of the
  forwarder unconditionally, whatever mode Neovim thinks it is in.
- **`wtype` can deliver the chord** (`-M ctrl -k grave -m ctrl` reproduced the
  E29 signature under cage), so a smoke can drive it.

**The exact price: the backtick spelling of `<C-@>`. One spelling survives and is
unbound — `Ctrl+Shift+2` → keyval `at` → `<C-@>`** (measured `action_for_key(at,
…)` is `None` for both masks). That is the same bargain `DECISIONS.md` already
documents for `Ctrl+Shift+<letter>`, and it matches macOS's `⌃\``.

The agent withdrew an overclaim rather than shipping it: Ctrl+Shift+grave is
**not** a second surviving spelling, because a real layout yields keyval
`asciitilde` for Shift+grave, giving `<C-~>` (`0x7E & 0x1F == 0x1E`), not `<C-@>`.

### M19. The restart hole is REAL, and it is this unit's sharpest hazard

SIGKILL the captive nvim with the drawer open → `liveness == Dead` and **the
snapshot freezes entirely**: grids, `windows`, `win_buffers`, `full_width`,
`active_win` all unchanged. Then `restart_nvim()` → `Ok(true)`:

| what | after restart |
|---|---|
| `grids` | `{1, 2, **4**}` — **the dead drawer's grid SURVIVES** |
| `windows` | `[(2,1000), **(4,1001)**]` — **its `WinInfo` SURVIVES** |
| `win_buffers` | `{1000}` only — wholesale-replaced |
| `full_width` | `{1000}` only — wholesale-replaced |
| intents | none |

**So `detect_closes` can never fire for the drawer** — it collapses a pane whose
grid vanished, and grid 4 does not vanish. And `reset_after_restart`
(`pane_view.rs:2863-2875`) clears `tree`, `panes`, `next_pane_id` and
`active_pane` — **a drawer held in a separate field, which O1 mandates, is not
among them.** Left unhandled, the front-end holds a stale drawer id pointing at a
dead grid whose stale contents are still in the snapshot and still paintable.

### M20. A separate, pre-existing bug the drawer makes far more likely

`open_files_for_tabpage` — the crash mirror the front-end replays after a
restart — returns the terminal buffer **and makes it the `active` entry**:

```
TabFiles { active: Some("term://…/bin/bash"),
           files: ["…/Untitled.md", "…/alpha.md", "term://…/bin/bash"] }
```

so `liveness_ui::reopen_order` would replay `InputEvent::Open { path: "term://…" }`
→ `:edit term://…`. **Reachable today without the drawer**, by typing `:terminal`
and crashing. What `:edit term://…` actually does to the pinned nvim is
**unmeasured**.

### M21. macOS's drawer statusbar — there is something to match, and it is poor

`paintStatusbar` has **exactly one** special case and it is preview; there is **no
drawer branch**. macOS therefore shows primary `bash` and secondary the full
`term:///…/<pid>:/nix/store/…/bin/bash` string, because `pane_status`'s
`display_path` tries cwd-relative, then home-relative, then falls through to the
raw string. **P1 is confirmed as parity, not a Linux quirk.** macOS does give the
drawer `kind: .editor` and full width, so its text sits flush left.

### M22. A doc nuance

`DECISIONS.md:135`'s "a terminal buffer forwards every key but one" describes
*stock* Neovim. `init.lua:976-979` adds four `t`-mode maps, so **in this
configuration it is every key but five**.

---

## Orchestrator decisions after the second reconnaissance — binding

**O9. Bind `<Control>grave`. O6 is discharged.** M18 verified every clause the
action row had assumed, and M18a settled the one that decided the design. The
price — the backtick spelling of `i_CTRL-@`, with `Ctrl+Shift+2` surviving — is
the bargain this repo's accelerator scheme already makes elsewhere, and it buys
parity with macOS's `⌃\``.

**O10. The invariant checker must learn the ctrl-folding class. In scope.**
`forbidden_reason` returning `None` for this row is **vacuous** — it short-circuits
on `!single_letter` before any rule runs, and it has no notion of ctrl-folding at
all. That gap is not academic: the same vacuous pass would wave through
`<Control>bracketleft`, which folds to `0x1B` — **Escape**. A checker that exists
to guarantee "no Neovim key becomes unreachable" and cannot see the class that
contains Escape is not doing its job.

Scope it tightly: a `Ctrl+<key>` chord whose keyval folds to a control byte must
declare the control code it consumes, so binding one is a deliberate, reviewable
act rather than a silent one. The architect proposes the exact shape; it must make
`<Control>grave` legal-with-a-declaration and `<Control>bracketleft` loud.

**O11. `reset_after_restart` must clear the drawer, and that clearing needs its own
test.** M19 makes this the unit's sharpest hazard and shows neither existing
mechanism catches it: `detect_closes` cannot fire, and `reset_after_restart` does
not know the field exists. A stale drawer id pointing at a dead-but-still-paintable
grid is exactly the shape of bug that survives a whole release.

**O12. M20's crash mirror is fixed here, in core.** A terminal buffer is not a file
to reopen, and the fix belongs where the mirror is built rather than in one
front-end. It is small, obviously correct, and this unit is what makes the bug easy
to hit. **Measure what `:edit term://…` does first** — the answer decides whether
the predicted red is "the replay fails" or "a shell is spawned per crash", and the
test must assert against what was measured, not what was assumed.

**O13. Ship the statusbar at parity — no drawer branch — and record that it is
poor.** M21 shows macOS has no special case, so matching it costs nothing and
adding one would be a Linux-only divergence in a unit whose whole subject is
parity. The `term://` display string is genuinely ugly, but prettifying it is a
`wolf_core::pane_status` change that would alter what macOS renders, with no macOS
host here to see it — the same trap unit 20's O6 recorded. Record it as a
cross-platform follow-up; do not fix it one-sidedly.

**O14. M19's macOS twin (B5) and M22's doc nuance are recorded, not fixed.** The
restart hole appears live on macOS today — `drawerPaneId` is cleared only in
`applyLayoutRebuild` and `detectCloses`, and the restart path
(`PaneContainerView.rebind(tabpageHandle:)`) clears neither — but that is
code-derived from a measured core behaviour with no macOS host to execute it.
`macos-followups.md` entry, phrased as inferred rather than measured.

---

## Orchestrator decisions on the architect's plan — binding, read before the plan

**The plan below is approved in full** except where these override it.

**O15. Caveat 1 is upheld — fix `DECISIONS.md:135` in place, overriding O14.** The
architect is right and I was wrong to file M22 as recorded-not-fixed. The sentence
sits **inside the accelerator-namespace paragraph A8 must rewrite anyway**, the
correction is one clause, and CLAUDE.md's first line makes a false doc a bug.
O14's "record, do not fix" reasoning applies to the *macOS-side* items it was
written for (B5, the `term://` prettifier) — things with no host here to verify.
A sentence about this repo's own `init.lua` is verifiable by reading it.

**O16. Caveat 2 — narrow the OCR rectangle before touching `PS1`.** S1's page
carries the raw `term://<cwd>//<pid>:…` string, which varies per run. M25 stays as
scheduled, but the **preferred mitigation is to sample OCR from the drawer's text
band only, excluding the statusbar strip** — the varying string then never enters
the sample. Changing `PS1` is second choice, because it changes the thing under
test; dropping the OCR leg is third and must be disclosed in `tests/README.md`.

**O17. Caveats 3 and 4 are accepted as written.** Test 10's non-isolated red
matches the existing convention in `command_action.rs` and is disclosed. The
drawer's idempotence guard **must not be made self-healing** — reading `panes`
instead of `DrawerState::is_open()` would delete S3's only falsifier, and the
guard's doc comment must say so.

**O18. Role boundaries override the architect's A0–A8 agent labels.** This repo
runs test-writers and coders as distinct actors: **the coder never writes an
assertion, and the test-writer never runs one.** Where a source file carries an
inline `#[cfg(test)] mod tests` (`drawer.rs`, `actions.rs`, `command_action.rs`,
`redraw.rs`), the coder writes the source in one pass and the test-writer adds the
test module in a later pass. They are serialized, so exclusivity holds.

**O19. No constant may be written before its measurement exists**, and no
assertion ships without a captured red. M23, M24, M25 and M27 are scheduled ahead
of the passes that consume them. Any smoke that moves against the 55-cage-smoke
baseline is a **finding to record, never a threshold to retune** — unit 20's O13,
carried forward.

### Pass table — replaces §6's agent order

| Pass | Actor | Files (exclusive) |
| --- | --- | --- |
| 1 | coder | **M23** (scratchpad probe, nothing committed), then `wolf-core/src/redraw.rs` filter, `wolf-gnome/src/drawer.rs` (source only), `wolf-gnome/src/lib.rs`. Ends `cargo build --workspace --all-targets`. |
| 2 | test-writer | `redraw.rs`'s test table (11), `wolf-core/tests/nvim_restart.rs` (12), `drawer.rs`'s test module (1–4). |
| 3 | coder | Run fast + core tiers. Capture reds for 1, 2, 3, 4, 11, 12, each falsifier alone; restore by `cp` baseline and checksum; re-run green. |
| 4 | coder | `actions.rs`, `command_action.rs`, `shell.rs` — the chord, the classification, the ctrl-folding rule. |
| 5 | test-writer | Tests 5, 6a, 6b, 7, 8, 9, 10 in those files' test modules. |
| 6 | coder | Run; capture reds; restore; green. |
| 7 | coder | `pane_view.rs` — the bulk. Ends with the **full-suite regression gate**: fmt, clippy `-D warnings`, `cargo test --workspace` against 969 / 94 / 55. |
| 8 | coder | **M24, M25, M27** — temporary smoke scaffolding, run, recorded, reverted to a clean tree and verified by checksum. |
| 9 | test-writer | `smoke_headless.rs` — S1, S2, S3 with the measured constants, plus the three fixture guards' new rows. |
| 10 | coder | Run the smokes; apply every falsifier one at a time, each alone; record verbatim; restore; verify; full gate. |
| 11 | reviewer | read-only |
| 12 | technical-writer | §9's table, plus O15's `DECISIONS.md:135` clause. |
| 13 | coder | Final gate: `prettier --write`, `markdown_check.sh`, fmt, clippy, full suite, lockfile and Bazel reconciliation. |

**Hazards carried from unit 20, binding on every agent:**

- **Never `git checkout --`, `git restore`, or `git stash`.** Work is uncommitted;
  a `checkout --` destroyed 1381 lines last unit and was recovered only by replay.
  Restore from a `cp` baseline taken first.
- **Never restore an `include_str!`'d asset with `cp -p`** — the preserved mtime
  makes Cargo reuse the cached build, so a checksum reads OK while the falsifier is
  still compiled in. Use plain `cp` plus `touch`, and re-run green before believing
  a restore.

## Status

Pass 1 done. Decisions O1–O22 binding.

### M23 — MEASURED. `:edit term://…` SPAWNS A SECOND SHELL.

Not an error, not an empty buffer. Driven through the production path
(`InputEvent::Open` against a fresh core, the post-restart analogue):

- **`ps` bash rows go 1 → 2 on the replay**, and back to 1 after `shutdown()`.
  Neovim's own `BufReadCmd term://*` autocmd re-runs `termopen()`.
- **The pid inside the name changes, `//2328133:` → `//2328211:`** — the conclusive
  discriminator. It is not the same buffer reopened, it is **a new job**. The name
  otherwise matches byte for byte, **including the dead core's cwd**, which no
  longer exists after a real crash.
- **No error, no toast, no message.** Nothing surfaces to the user.
- **No new grid or window** — the terminal *displaces the file* in the pane it
  lands in (`win 1000` went `Untitled.md` → `term://…`), and `is_drawer` is
  **false**: a terminal squatting in an editor pane.
- **It compounds.** The replayed buffer re-enters `open_files` immediately, so each
  crash+replay cycle spawns another shell and re-poisons the mirror. Because
  `reopen_order` puts `active` **last so it lands focused**, and M20's `active` *is*
  the terminal, a post-crash window ends **focused on a freshly spawned shell**.
- **Control leg**: an ordinary missing path opens an empty named buffer and spawns
  nothing, so this is specific to the `term://` scheme.

Pass 2's doc comment says **"a shell is spawned per crash"**, not "the replay
fails".

### Pass 1 — coder — DONE

`redraw.rs` (+44/−2) carries `TERMINAL_BUFFER_PREFIX` and
`is_terminal_buffer_name`, applied to **both** halves before `canon`.
`drawer.rs` (new, 306 lines) is pure and GTK-free, depending only on
`wolf_core::FocusDir` and `crate::layout::{PaneId, PaneTree, Rect}`. `lib.rs` gains
the module and one seam bullet. Build, clippy `-D warnings`, fmt all clean; 271 lib
tests and 6 real-nvim tests green. Nothing in the plan was contradicted.

**O20. Four pass-1 findings bind on pass 2.**

- **`drawer_split` guards `weight` and `min_drawer_height` for non-finiteness too**,
  broader than §2's literal wording, following `editor_inset::grid_rect`'s stated
  precedent. Narrowing it would be **worse**: a non-finite `weight` with
  `has_drawer` true makes `round(h * NaN).clamp(..)` return `NaN`, so both fields
  would come back `NaN` rather than zero. **Pass 2's table must match the shipped
  behaviour**, not §2's wording.
- **The lower clamp bound is `min_drawer_height.max(0.0).min(pane_area_height)` — a
  panic guard, not tidiness.** `f64::clamp` panics outright on an inverted range,
  and a pane area shorter than one pane's minimum is a real transient while an
  allocation settles. Same shape as `geometry::divider_position`, whose panic was
  measured in an earlier unit.
- **The mirror for a drawer-open session now has 2 entries, not 3**, and its
  `active` is `None` when the drawer holds focus. **Test 12 must be written against
  that shape**, not M20's.
- **`is_terminal_buffer_name` and `TERMINAL_BUFFER_PREFIX` are private**, so only an
  inline `#[cfg(test)] mod tests` inside `redraw.rs` can reach them. A test in
  `crates/wolf-core/tests/*.rs` could not.

**O21. Five extra test rows are ACCEPTED into pass 2**, each with a real falsifying
source edit the plan's §5.2 table does not yet name:

- `drawer_split_table`: a **floor row** (`drawer_split(90.0, true, 1/3, 63.0)` →
  drawer 63, editor 27; falsifier drops the `low` bound) — without it divergence 1
  is documented but unasserted.
- `drawer_split_table`: an **inverted-range row** (`min_drawer_height >
  pane_area_height`); falsifier drops the `.min(pane_area_height)` cap and the
  **predicted red is a panic from inside the standard library**, not an assertion
  failure — the same shape `geometry::divider_position_table` already records.
- `drawer_split_table`: **non-finite `weight` / `min_drawer_height` rows**, pinning
  O20's broadened guard rather than leaving it incidental.
- `bottom_most_leaf_table`: rows for **both** tie-break levels — equal `y+h` with
  different `x`, and equal `y+h` **and** equal `x` with different ids. The second is
  a verbatim-port clause and this is the cheapest way to stop it being deleted as
  unreachable.
- `terminal_buffer_names_are_recognised`: a **negative row** where a real path
  contains `term://` somewhere other than the start, pinning `starts_with` against a
  later "simplification" to `contains`.

**O23. Pass 2 caught a VACUITY in §5.3's scenario. Its fix is accepted.** The
plan's one-toggle version could have passed **with the filter deleted**:
`:terminal` enters a **brand-new unnamed** buffer, and `init.lua`'s `listed_files()`
drops unnamed buffers (`init.lua:1437`), so the `wolf_open_files` emit fired during
the drawer build reports 2 files and no active path — **byte-identical to the
filtered answer**. No level-triggered predicate over the mirror can tell "filtered
out" from "the emit has not seen the name yet", so polling could not close it
either.

The shipped scenario is **open → hide → re-open**. `wolf_toggle_terminal` keeps the
terminal buffer and job alive across a hide (M8), so the re-open takes the
`nvim_win_set_buf` reuse branch and fires `BufEnter`/`BufWinEnter` on a buffer that
**already carries `term://…`** — both are in the open-files autocmd list, so the
emit definitely sees the terminal and only the filter can keep it out. Cost: two
extra toggles against `nvim_restart`'s unmeasurable `size = "small"` headroom —
add it to that ledger row.

**O24. O20's third bullet is CORRECTED by pass 2's reading.** I wrote that
narrowing `drawer_split`'s guard "would return `NaN` in both fields". That is true
only of the **`weight`** half. The `min_drawer_height` half behaves differently:
`f64::max` ignores `NaN`, so dropping it yields a **silently lost floor**
(200/400), not `NaN`. The two falsifiers therefore have **different** predicted
reds, and pass 3 must capture the right one for each.

**Pass 3 — coder — DONE, all fourteen falsifiers plus thirteen extra reds
captured.** `wolf-gnome --lib` **275 passed**, `wolf-core --lib` **224 passed**,
`nvim_restart` **3 passed**; fmt and clippy `-D warnings` clean. `cargo fmt`
reflowed two `poll_until` closures in `nvim_restart.rs` and left all three source
files byte-identical.

- **O23 is VINDICATED — falsifier 14 genuinely reddens.** With the filter deleted,
  the re-opened already-named terminal buffer reaches the mirror as both the active
  entry and a third file, M20's exact shape. The rewritten scenario is **not
  vacuous**, which the one-toggle version would have been.
- **O24 is CONFIRMED by measurement**, having been reasoned in pass 2: dropping the
  `min_drawer_height` finiteness guard returns **200/400, not NaN** — the floor is
  silently lost. **O20's third bullet as originally written was wrong.**
- **Both halves of the `apply_open_files` filter are independently load-bearing** —
  filtering only `files` reddens the `active` assertion; filtering only `active`
  reddens the no-`term://`-entry assertion. The test catches each alone.
- Two predicted reds are **not assertion failures** and both landed as described:
  the `f64::clamp` panic from inside the standard library, and the silent floor
  loss. Nothing pass 2 wrote is contradicted.
- **`nvim_restart` wall clock: 2.39 s serialized** (2.16 s parallel), essentially
  all of it the 2 s `MIRROR_SETTLE`, against Bazel's 60 s `size = "small"` budget.
  O23's two extra toggles cost nothing measurable. Cargo measurement only — Bazel
  adds action overhead unmeasurable here.

**O25. Pass 3's recommendation 1 is ACCEPTED into pass 5.** `drawer.rs`'s **third**
documented divergence — that `focus_target` takes the **raw pane-area size** because
`pane_rects` feeds only ordering and both consumers are scale-invariant — has **no
assertion anywhere**. It is exactly the claim a reader diffing against
`PaneContainerView.applyFocus` would "fix" by threading `editor_height` through, and
nothing today would fail if they did. Cheap closure: run the existing
`focus_target_table` cases a second time at a reduced height (e.g. `PANE_H * 2/3`,
the editor region an open drawer leaves) and assert **every answer is unchanged**.

**Pass 4 — coder — DONE and green.** `actions.rs` carries `ctrl_fold`, `CtrlFold`,
`CTRL_FOLD_FORBIDDEN`, the extracted+widened `key_symbol`, and `win.terminal`
promoted to `Bound` with a `Terminal` menu row; `command_action.rs` classifies
`Command::Terminal` to the new `UiAction::ToggleTerminal`; `shell.rs` dispatches it.
275 lib tests, build/clippy/fmt all clean. §7's assumption **held** — the only two
`forbidden_reason` callers are inside `actions.rs`'s own test module.

**The chord is live end to end, confirmed by hand under `cage`**: `wtype -M ctrl -k
grave -m ctrl` opened the drawer (`2 terminal term:///…/bin/bash` read back), with
**no terminal strip anywhere** in the `grim` capture — M15/M16's page reproduced
through the bound chord. That is the invisible shell this pass deliberately leaves
for pass 7.

The `CTRL_FOLD_FORBIDDEN` arm was proved **three ways**: a temporary
`<Control>bracketleft` row declaring `None` is refused; the same row declaring
`Some(0x1B)` gets a **byte-identical** refusal; and deleting the arm with that row
present makes the test **pass** — the chord folding to Escape got through, test 6b's
exact predicted vacuity.

**O26. Pass 4's `!single_letter` gate on the declaration rule is ACCEPTED, but its
stated reason is incomplete and must be corrected.** Without the gate, every
`Ctrl+Shift+<letter>` row would demand a declaration, contradicting "every existing
row declares `None`". The coder justified it as "no ASCII letter folds onto
`CTRL_FOLD_FORBIDDEN`" — true, but it does not explain why consuming `0x09` (Tab,
via `Ctrl+Shift+I`) or `0x0D` (Enter, via `Ctrl+Shift+M`) is acceptable.

**The real reason is the modifier mask.** Rule 1 already refuses a *bare*
`Ctrl+<letter>`, so every surviving letter row carries **Shift** — and
`action_for_key` matches masks **exactly**, so a Shift-carrying accelerator does not
intercept the unshifted `Ctrl+<letter>` event at all. Neovim still receives `<C-I>`
and `<C-M>`. That is also precisely why `<Control>grave`, which carries **no**
Shift, genuinely does consume `<C-\`>` while `Ctrl+Shift+2` survives (M18).

Correct the comment to say this, and pin it: pass 5 adds a row asserting
`<Control><Shift>o` with `declared = None` is **accepted** while `ctrl_fold` for it
answers `Byte(0x0F)`. Otherwise a reader deletes the gate, sees nothing fail in the
module, and discovers the twelve-row breakage only by running the table.

**O27. Pass 4's four test recommendations are ACCEPTED into pass 5.**

- **Test 6b must declare `Some(0x1B)`, not `None`.** Measured both ways: with
  `None`, deleting the `CTRL_FOLD_FORBIDDEN` arm still reddens via the
  missing-declaration rule, so the falsifier would **not isolate that arm**.
- **`ctrl_fold_table` needs `NoFold` and `UnknownKey` rows, not only `Byte`.**
  `<Control>0` / `equal` / `minus` are the live `NoFold` rows — nothing currently
  asserts the three Reserved font-size chords stay legal with `ctrl_fold: None` —
  and `UnknownKey` is reachable only through a name absent from `key_symbol` (e.g.
  `<Control>Left`), the arm whose whole purpose is non-vacuity. Add a
  `question` → `Byte(0x7F)` row too: `?` is the one symbol not covered by `& 0x1F`
  and nothing else pins it.
- **Test 7 must disclose its non-isolation**, as test 10 does: demoting
  `win.terminal` to `Reserved` reddens `menu_never_lists_a_reserved_action` beside
  `bound_set_is_exactly_this_unit`, because the row now carries a menu item.
- **O26's gate row**, above.

**O28. One doc nit for pass 12**: `command_action.rs:226`'s test-module doc says the
two unimplemented classes carry "~18 sentences" — already loose, and now one reason
shorter.

**Pass 5 — test-writer — DONE.** `ctrl_fold_table` (11 rows, +1 test function, so
`--lib` should go 275 → 276), `no_accel_makes_a_neovim_key_unreachable` extended,
`action_for_key_table` +4 rows, O25's second pass over `focus_target_table`, and
row 10's non-isolation disclosure. Three additions **beyond** the plan, each
closing a hole it found:

- **`CTRL_FOLD_FORBIDDEN`'s second entry (`0x7F`) had NO assertion at all** —
  dropping `0x7F` from the array reddened nothing. Now pinned by a
  `<Control>question` / `Some(0x7F)` row.
- **The wrong-declaration branch had no assertion** — only the missing-declaration
  one did. Now pinned by `<Control>grave` / `Some(0x1F)`.
- `action_for_key_table` gains `Ctrl+asciitilde` → `None`, which is what a **real
  keymap** delivers for that press (M18), so the negative covers a reachable
  spelling rather than only a synthetic one.

**O25's limit is disclosed honestly rather than overclaimed**: no edit to
`focus_target` itself reddens the second pass, because passing a proportionally
smaller height is precisely the change the divergence says is unobservable — that
*is* the claim. What it converts is the unverified premise that
`pane_rects`/`neighbor` scale with their bounds, measured against the real
`PaneTree`, with three fixture guards ensuring the reduced height produces
**different** rects so the second pass cannot be a re-run of the first.

**O29. Two comment defects for pass 12, both found by pass 5, neither touched.**

- **`menu_never_lists_a_reserved_action`'s message states a false mechanism.** It
  says a reserved action "is not installed this unit, so the item would appear in
  the primary menu and do nothing when clicked". But `install_actions`
  (`shell.rs:614-673`) installs a `SimpleAction` for **every** row whose command
  classifies to a real `UiAction`, regardless of `status` — only accelerator
  registration and `action_for_key` are gated on `Bound`. The menu item would
  **work**. The rule that test enforces is a policy, not that consequence.
  Pre-existing and unrelated to this unit; pass 5 correctly declined to edit it
  mid-pass so as not to muddy pass 6's red attribution.
- **O26's corrected reason is still not in the source.** `actions.rs:52-53`'s module
  doc states the incomplete character-class version, and `forbidden_reason`'s inline
  comment gets closer but never says that `action_for_key` matches masks **exactly**
  — which is the whole reason consuming `0x09`/`0x0D` costs nothing. Pass 5 wrote
  the full reason into the test comment as instructed; the two source comments are
  pass 12's.

**Pass 6 — coder — DONE. 276 passed** (275 → 276 exactly as predicted), fmt and
clippy clean. All twelve falsifiers captured verbatim, each alone, all restores
checksum-verified. **6c confirmed**: dropping `0x7F` from `CTRL_FOLD_FORBIDDEN` now
reddens, where pass 5 measured it reddening nothing before its row existed. 6e fires
first on `win.go-to-file` / `<Control><Shift>o`. The 5b, 5e and 9/10 pairings all
confirmed.

**O30. Three prose/structure defects pass 6 measured. Fixed in pass 6b.**

- **The O26 gate row is SHADOWED and can never produce its own red.** It sits
  *after* the `ACTION_TABLE` loop inside the same `#[test]`, and under the only
  falsifier that could reach it (6e, deleting the gate) the loop panics first on
  `win.go-to-file`. **It documents O26's reason correctly but is not load-bearing** —
  documentation wearing a test's clothes. Split it into its own `#[test]`, or move
  it above the loop.
- **`ctrl_fold_table`'s mask-widening bullet omits its non-isolation.** Measured:
  widening `& 0x1F` → `& 0x7F` also reddens
  `no_accel_makes_a_neovim_key_unreachable` on the wrong-declaration rule, because
  `<Control>grave` then folds to `0x60` against a declared `0x00`. The two bullets
  around it both carry a "NOT isolated" note; this one must too.
- **`bound_set_is_exactly_this_unit`'s disclosure is one test short.** It names only
  `menu_never_lists_a_reserved_action`; `action_for_key_table` reddens too — caused
  by pass 5's own new `Ctrl+grave` row, so pass 4's disclosure was correct when
  written and pass 5 invalidated it without updating it.

**Pass 7 — coder — DONE. THE BEHAVIOUR LANDS.** All twelve §3.5 items shipped in
`pane_view.rs` (+278/−17). **`cargo test --workspace` 977 passed / 0 failed / 1
ignored, 94 suites, `smoke_headless` 55 passed** — fmt and clippy clean, no
`#[allow]` added. Delta **+8** accounted exactly to passes 1–6b (pass 7 adds zero
tests); §5 predicted +7, the extra being pass 6b's O30 split. §3.5's assumption
about the host-unparent loop **holds** — the drawer is an ordinary `panes` entry.

**No cage smoke moved** — 55 passed, 440.49 s against the baseline's 440.85 s.
Structural reason: `apply_drawer_frame` and the append are both `None`-guarded on
`drawer.get().id()`, so with no drawer bound the pane area is byte-for-byte the tree
widget it was.

**M26 — the by-hand `cage` run, four things settled live:**

- **The strip is there.** Pass 4's capture showed none; this one shows a full-width
  drawer with its own statusbar, a focus ring, a live prompt, and the typed marker
  and its output. M15/M16's invisible shell is closed.
- **The focus bridge works both ways** — `<C-w>k` lands in `alpha.md` with empty
  `&buftype`, `<C-w>j` lands back in the terminal. **M11's trap is closed.**
- **Row E holds with NO front-end code, as O4 predicted**: editor `winwidth = 90`
  (exactly `MAX_GRID_COLS`) against drawer **123**. S2 leg c should pass as written;
  nothing needs widening.
- **O5 holds**: `winheight = 8` against `&lines / 3 = 13`. **O2 holds live**: with
  the drawer open at quit, `state.json` has one leaf and **zero** `term://`.

A second run split with the drawer bound: two editor panes **and** the strip, no GTK
criticals, no double-parented host. A `:split` from inside the drawer left the
front-end at two panes plus the strip — item 6's guard dropping it rather than
inserting an invisible pane.

**Pass 7's twelve items currently have NO automated falsifier** — their only evidence
is that by-hand run. Passes 9 and 10 convert it. The coder stated this plainly rather
than dressing it up.

**O31. S1's OCR marker leg CANNOT work at `SMOKE_FONT_PT`. Pass 8 must settle it
before any assertion is written.** Measured: at font 28 the drawer's grid is **one
row** (`winheight=1`, `lines=13`), so the planned `echo <MARKER>` + Return leaves the
marker scrolled off — the font-28 capture shows a bare prompt. At font 10 the same
scenario shows it plainly with 8 rows. S1 needs a different font, a marker that
survives at one row, or a non-OCR leg. This is M25's business.

**O16 is CONFIRMED necessary.** Both the statusbar's raw
`term:///<abs cwd>//<pid>:/nix/store/<hash>/bin/bash` **and** the shell prompt (full
absolute cwd plus literal `\[ \]` escapes) vary per run and both sit inside the
strip. An OCR sample of the whole drawer is a flake by construction; sample the text
band only — and note the prompt line still varies within it.

**O32. Two of pass 7's four recommendations are ACCEPTED into S1; two become
unproven rows.**

- **ACCEPTED — the drawer survives an ordinary `rebuild_pane_area`.** S1 already
  opens a fixture; add a `:vsplit` and assert the drawer is still reachable
  (`<C-w>j` → `&buftype` is `terminal`). One extra readback, same launch. It is the
  cheapest way to redden a future change that appends the host wrongly or forgets
  the unparent.
- **ACCEPTED — `apply_split`'s new drawer guard has no coverage**, and deleting it
  produces an invisible pane bound to a real window: silent, and invisible to every
  existing assertion. A `:split` from inside the drawer plus a window-count readback
  pins it; pass 7 measured the shape (`drawersplit wins=4`). Same launch.
- **UNPROVEN ROW — `apply_drawer_frame`'s `min_drawer_height` floor.** `drawer.rs`'s
  table pins `drawer_split`'s arithmetic, but nothing pins that `pane_view` passes
  `editor_inset::min_pane_size(..).1` rather than `0.0`, and that falsifier is
  invisible at every geometry the smokes run at. Record it; do not invent a leg.
- **UNPROVEN ROW — the two-frame height convergence on a window resize**, already
  named in the plan: nothing under `cage` can resize the toplevel.

## Pass 8 measurements — MEASURED FACT. Four plan premises falsified.

Tree ended clean, `sha256sum -c` OK, 55 tests in `smoke_headless`, 0 probes left,
no leaked processes after 15 cage launches.

### M25/O31 — SETTLED. Keep `SMOKE_FONT_PT`; change the marker COMMAND.

**`printf <MARKER>; sleep 300` + Return**, captured while still in terminal-insert,
OCR'd from the band below the drawer's statusbar. No font change, no `PS1` change.

| leg at `SMOKE_FONT_PT` | band OCR | marker read |
| --- | --- | --- |
| planned `echo <MARKER>` + Return | `…jplein@nixos:~/project1$…` | **no**, 6/6 |
| `printf <MARKER>; sleep 300` | `zzdrawermark]` | **yes**, 3/3 |

At 28 pt the drawer is one text row, so `echo` scrolls the marker off. `printf`
without a newline followed by a long-blocking command means the shell never prints
another prompt, so the marker is the last thing on the last row. Band md5
`5754448133…` on all three runs.

**Font 16 is REJECTED even though it works** — at 16 pt the band also contains the
**echoed command line**, so `contains(marker)` would be satisfied by the typed
characters alone: **a vacuity the one-row regime does not have.** It would also put
S1 outside `the_column_cap_cannot_bind_at_the_ocr_smokes_geometry`,
`cell_metrics_for_smoke` and `SIDEBAR_FILLER_COUNT`'s calibration. Font 10 degrades
OCR visibly.

**Hazard: take the capture BEFORE `<C-\><C-n>`.** In normal mode the block cursor
sits *on* the last character and tesseract misread `zzdrawermark` as
`zzdrawermarj`.

### O16 — the band, and PASS 7'S PROMPT CLAIM IS FALSIFIED

Band `y 560..674` at `SMOKE_FONT_PT`. **The prompt does NOT vary per run**:
`cage_command` sets `HOME` to the temp cwd's parent, so bash's `\w` renders the
constant `~/project` — the same mechanism that pins the window title.

| sample | md5 across three runs |
| --- | --- |
| whole page / drawer strip / statusbar alone | **different every run** (the `//<pid>:` digits) |
| **band below the statusbar** | **byte-identical**, 6/6 and 3/3 |

Derivation, exact at 28/16/10 pt: `pane_area_h = 720 − 47 − (cell_h + 2)`;
`strip_top = 720 − (cell_h + 2) − round(pane_area_h/3)`; band =
`[strip_top + statusbar_h, 720 − (cell_h + 2))`. **No band-crop helper exists in the
file** — pass 9 needs one, reading the band from the *same* capture instant rather
than taking a second `grim`.

### M24 — geometry. S2's three legs all HOLD.

| font | drawer `winwidth(0)` | drawer `winheight(0)` | editor `winwidth(1)` | `&lines` |
| --- | --- | --- | --- | --- |
| 28 (`SMOKE_FONT_PT`) | 44 | 1 | 44 | 15 |
| 10 (`FULLWIDTH_SMOKE_FONT_PT`) | **123** | **8** | **90** | **42** |

b: 90 == `MAX_GRID_COLS`. c: 123 > 90. d: 8 < 14. **At `SMOKE_FONT_PT` leg b would
FAIL (44 ≠ 90)**, so S2 must keep its own font exactly as §5.4 says. `&lines` is
**42**, not pass 7's 40, so `&lines/3` is 14 — leg d compares two live values, so
unaffected.

### M27 — §5.1's NON-VACUITY PREMISE IS WRONG

10/10 runs: exactly one leaf, the fixture's path, **no `term://` anywhere**. But with
the `:e` removed the layout **still persists** as one leaf naming `Untitled.md` —
`persistable_layout` suppresses an *unnamed* lone leaf and the app's default buffer
**is** named. So "one leaf, no `term://`" is **not vacuous without a fixture**. The
fixture is still needed for the other half — that the leaf **is** the fixture's path
— but the stated reason must be corrected, not repeated.

### O32 leg 2 — ITS READBACK IS REFUTED

`len(tabpagebuflist()) == 4` **does not distinguish guard-held from guard-deleted**:
`init.lua`'s `do_split` runs `vim.cmd("split")` **before** emitting the intent, so
Neovim has four windows in both builds, and `pending_resize` returns `None` for a
zero-sized allocation so the invisible pane issues no `resize_grid` either.

**The discriminator is `<C-w>k` from the window the `:split` created inside the
drawer**, then `&buftype`: measured green `bt=[]` 10/10; predicted red `bt=[terminal]`
(with the guard gone `pane_for_win` is `Some(invisible)`, `focus_target` takes the
editor branch, `neighbor` returns `None` because the pane is not a leaf, and nothing
moves). Secondary pixel discriminator: the 2-px active outline sits on the drawer
when the guard holds and nowhere when it does not — **but only in a capture taken
before any `<C-w>k`**, so the ordering is load-bearing.

Leg 1 (`:vsplit` with the drawer bound) **holds 10/10**, structurally — the strip
occupies the same rows after the split.

### S3 — §5.4's PREDICTED RED IS WRONG

| step | measured |
| --- | --- |
| kill → recovered | **1.007 s / 1.016 s** |
| chord #2 → drawer answers `bt=[terminal]` | **810 ms / 812 ms** |
| whole scenario | 10.6 s |

§5.4 predicts "one window and an empty buftype". **False**: the chord runs
`_G.wolf_toggle_terminal()` *inside* Neovim, so with the clear deleted Neovim still
builds the drawer window — `wins=2`, `&buftype == terminal`, in **both** builds.
Only the front-end half breaks. The discriminator is again **`<C-w>k` out of the
re-opened drawer**: `bt=[]` fixed, `bt=[terminal]` broken. Poll, do not sleep —
`FILE_TRIES=100 × 0.2 s` is ~20x the measured recovery, and both polls succeeded on
their **first** iteration in both runs.

**Pass 8 could not run the two `src/` falsifiers** (it may not touch `src/`), so
both are derivations with source citations. **Pass 10 owns capturing them.**

## Passes 9 + 10 — DONE. IMPLEMENTATION COMPLETE AND GREEN.

**`cargo test --workspace` 980 passed / 0 failed / 1 ignored, 94 suites,
`smoke_headless` 58 passed.** Delta **+3**, accounted exactly to pass 9's three
smokes. **No pre-existing test moved** — O13/O19 has nothing to record. fmt and
clippy `-D warnings` clean. All three smokes passed on their **first** execution.

**All nine falsifiers produced verbatim reds, each alone, all restores
checksum-verified.** The two that matter most:

- **S1-c proves O1 was load-bearing, not aesthetic.** Inserting the drawer via
  `tree.split(..)` — the design O1 rejected — persists a **two-child horizontal
  split with `term://~/project//<pid>:…/bin/bash` as the second leaf**. P2 realized,
  in `state.json`, exactly as predicted.
- **S3 confirms pass 8's correction and proves §5.4 wrong.** Under the falsifier the
  first three readbacks are **byte-identical to green** (`bt=[terminal] wins=2`,
  `bt=[] wins=1`, `bt=[terminal] wins=2`); only `<C-w>k` out of the re-opened drawer
  discriminates. Neovim rebuilds the drawer window in **both** builds, so §5.4's
  "one window and an empty buftype" would never have fired.
- **O32 confirmed under measurement**: the window count stayed **4** under S1-g's
  falsifier, identical to green — a precondition that cannot discriminate.

**Pass 9's three uncertainties all CONFIRMED**: `--psm 11` reads
`ZZDRAWERMARK]`; the band at 557 carries no statusbar ink (its entire OCR output is
that one token); leg f's window-count precondition is a real live-vs-live comparison
(2 → 3).

**O33. S1's leg table mis-attributes one falsifier. Pass 12 fixes it.** Leg f's row
claims that appending the drawer host **before** the tree widget makes the strip
"gone or unreachable". Measured, it reddens legs **b and b'** instead: the strip
simply moves to the **top** of the pane area, so the bottom-derived band samples
editor rows. Leg f is never reached under that edit and would probably survive it,
since the focus bridge is a pure seam over `DrawerState` with no knowledge of widget
order. Re-attribute the row to b/b', and say what leg f **alone** can catch
(dropping the unparent loop, or the pane being unbound by the rebuild) — noting both
are untested against a captured red.

**O34. `flake.nix:345` needs BOTH numbers, not one.** It reads "forty-eight headless
end-to-end smoke tests … of that file's fifty-five tests". §3.7 asked only for
forty-eight → **fifty-one**; the same sentence's "fifty-five" must become
**fifty-eight**. Pass 12's, and note that touching `flake.nix` re-evaluates the
devShell.

**Two unproven rows stand and are unconvertible here**: `apply_drawer_frame`'s
`min_drawer_height` floor (invisible at every geometry the smokes run at), and the
two-frame height convergence on a toplevel resize (nothing under `cage` can resize
the toplevel).

## Pass 11 — reviewer — VERDICT: needs revision, on A1 alone.

O1–O34 all verified honoured against the code, with citations. The three smokes'
discriminators are real; S1-c and S3 called exemplary. No correctness concern in
shipped behaviour. `flake.nix` confirmed **untouched** — the session's ` M flake.nix`
snapshot is stale, from a different branch.

**O35. A1 is a real gap and is FIXED here.** §3.5 **item 8** — `detect_closes`'s
drawer arm — has **no falsifier and no unproven row**, and it is reached by the
single most common action after opening the drawer: **pressing `<Control>grave`
again to hide it.** Passes 9/10 converted nine falsifiers covering eleven of the
twelve pass-7 items; item 8 is the one left neither converted nor disclosed.

The arm is load-bearing, not defensive. With the special-case block and its `filter`
removed, and one editor pane on screen (the ordinary case), the generic loop's
`if tree.leaves().len() <= 1 { break; }` fires **immediately** — the drawer is never
removed from `panes` and `DrawerState` is never cleared. Result: **a ghost pane
painting a destroyed grid, and `apply_drawer_open`'s `is_open()` guard makes the
chord a one-shot for the rest of the session** — O11's "bug that survives a whole
release", reached **without a crash**.

Fix: extend `DRAWER_SCENARIO` after step 7, where focus is already in the drawer and
the arrangement is one editor pane + drawer, so steps 8/9 are unaffected — hide,
re-open, `<C-w>k`, probe. Green `bt=[]`; predicted red `bt=[terminal]`. Falsifier:
delete the `if let Some(drawer_id) = closed_drawer { … }` block **and** the loop's
`.filter(…)` from `detect_closes`.

**O36. A2 is RECORDED as an unproven row, not converted.** `set_active_pane`'s
`record_active` wiring (§3.5 item 9) is unfalsified: deleting the call leaves every
assertion green, because in S1 the MRU and `bottom_most_leaf` answers **coincide** at
every point a `<C-w>k` is issued — one editor leaf before the `:vsplit`, and after it
`:vsplit` focuses the new **left** window while `bottom_most_leaf` ties on the bottom
edge to the smallest `x`, also the left. `focus_target_table` pins the *rule*;
nothing pins that `pane_view` calls it.

The failure mode is a **cosmetic MRU regression, not a trap**, and S1 is already
gaining A1's legs. The reviewer's recipe is recorded so a later unit can take it
cheaply: after the `:vsplit`, `<C-w>l` → `<C-w>j` → `<C-w>k`, reading back `winnr()`
— green answers the right pane, red the left.

**O37. A3 and A4 are doc-only.** `apply_layout_rebuild`'s `clear_drawer()` is
**unreachable** — `consume_pending_layout` runs at launch only, gated on exactly one
existing pane, so no drawer can be bound when it fires. Correct defensive code; one
clause saying so, not a test. And S1 **leg f** joins the unproven rows alongside
O33's re-attribution.

**O38. Pass 11's comment findings B1–B8 go to pass 12. B9 is already fixed — do not
redo it** (my ledger was stale; `menu_never_lists_a_reserved_action`'s message was
corrected in pass 6b).

- **B1** `smoke_headless.rs:28259` — says `--psm 11` "IS a premise rather than a
  measurement". **Pass 10 measured it** (`ZZDRAWERMARK]`). Now false; state the
  measurement.
- **B2** `drawer.rs:122` — "six arguments rather than seven, which is where
  `clippy::too_many_arguments` bites". The lint fires when the count **exceeds** the
  threshold (default 7), i.e. at eight. The decision is fine; the reason is wrong.
- **B3** `actions.rs:1997` (`the_about_row_carries_no_chord`) — carries the identical
  false mechanism O29 flagged and pass 6b corrected in its sibling. `win.about`
  resolves through `ui_action_for_front_end_action`, so a Reserved About row **would**
  be installed and **would** work. Rewrite in the corrected sibling's shape.
- **B4** `actions.rs:527` — "a Reserved row's action is not installed this unit" is
  true only because every current Reserved row classifies `Deferred`; it is stated as
  a consequence of `Reserved`. Pre-existing, lower priority.
- **B5** `pane_view.rs:3094` — "the one structural thing in this file that knows the
  drawer exists" is falsified two lines later by `apply_drawer_frame`. Narrow to "the
  one place the widget **tree** knows".
- **B6/B7/B8** — O29 bullet 2 (the mask-exactness reason still absent from both source
  comments), O28's "~18 sentences", O33's leg-f row.
- **Plus**: `actions.rs:700` states `Ctrl+?` → DEL as fact. It rests on convention and
  was **never measured here**, unlike `<Control>grave` which was measured three ways.
  The direction of error is safe (it only *refuses* a chord nothing binds). Say "by
  convention, not measured here".

**O22. Test 12's doc comment carries M23's measured answer verbatim** — a second
shell spawned, fresh pid in the same name, no error and no message — because that
is what makes the filter's importance legible to the next reader. Its predicted red
stays M20's exact `TabFiles`.

---

# Work unit 21 — implementation plan

`M*`/`R*`/`D*`/`P*`/`O*` refer to the reconnaissance and decisions above.

## 1. Summary

The Neovim half of both rows is already live on Linux (M4/M5), so this unit builds
the GTK half only: a **terminal drawer held outside `PaneTree`** (O1) as a second
child of the existing vertical `pane_area` box, bound on
`LayoutIntent::DrawerOpen`, sized from its own GTK allocation through the same
`resize_grid` funnel every pane uses (O5), focusable both ways through a ported
pure seam (O3), excluded from `state.json` by construction with a smoke assertion
to prove it (O2), cleared on restart (O11), and reachable from `<Control>grave`
after promoting one action row and one command classification (O9) behind a new
ctrl-folding invariant rule (O10). Two pre-existing defects are fixed alongside:
the false comments at `pane_view.rs:924`/`:929` (O7) and the crash mirror that
replays a terminal buffer as a file (O12).

## 2. New file — `crates/wolf-gnome/src/drawer.rs`

The pure, GTK-free seam. It **merges macOS's two files** (`DrawerLayout.swift`,
`DrawerFocusSeam.swift`) into one module — both are ~40 lines and neither has a
consumer outside the drawer. That merge is a divergence to state in the README.

**Types.** `pub const DRAWER_WEIGHT: f64 = 1.0/3.0` (matching `init.lua:1231`'s
`floor(lines/3)`). `pub struct DrawerSplit { editor_height, drawer_height }`.
`pub struct DrawerState` — `Copy`, two private `Option<PaneId>` fields (the bound
drawer pane, the MRU **editor** pane), with `new/id/is_open/is_drawer/bind/clear/
record_active/last_editor`. **`clear()` empties both fields** — it is what
`reset_after_restart` and `apply_layout_rebuild` call. `record_active` records only
an editor leaf that is not the drawer (macOS parity,
`PaneContainerView.swift:2801-2809`).

**Functions.**

- `drawer_split(pane_area_height, has_drawer, weight, min_drawer_height)` —
  non-finite or non-positive height collapses **both** fields to `0.0` (this
  crate's `geometry.rs`/`editor_inset.rs` convention); `has_drawer == false` gives
  the whole height to the editor; otherwise `round(h * weight)` clamped into
  `[min_drawer_height, h]`, editor takes the remainder.
  **Two divergences from Swift, both strictly more defensive**, to be recorded like
  `cell_metrics_math.rs` records its own: the `min_drawer_height` floor (Swift has
  none, so a short window yields a drawer thinner than one grid row) and the
  non-finite guard.
- `bottom_most_leaf(rects)` — greatest `y + h`; ties to smallest `x`, then smallest
  `PaneId`. A verbatim port.
- `drawer_up_target(state, leaves, rects)` — the MRU editor leaf when still in
  `leaves`, else `bottom_most_leaf`.
- `focus_target(tree, origin, dir, state, width, height)` — the whole O3 bridge as
  one value. Drawer origin: `Up` → `drawer_up_target`, every other direction →
  `None` (the drawer is full width). Editor origin: `tree.neighbor(...)` when
  `Some`; else `Down` with an open drawer → the drawer; else `None`.

**Deliberate divergence to record.** macOS resolves `neighbor`/`paneRects` over
`DrawerLayout.editorHeight`. Linux passes the **raw pane-area size**, because here
`pane_rects` feeds only *ordering* (`neighbor`'s comparisons, `bottom_most_leaf`'s
max) while GTK computes the real geometry — both consumers are scale-invariant for
every non-degenerate tree, so threading `editor_height` would be unfalsifiable
precision. Say so, or a reader diffing the two files will "fix" it.

## 3. Source changes

### 3.1 `lib.rs`

`pub mod drawer;`, plus one seam-inventory bullet in the crate doc in the style of
`refresh_coalescer`/`tree_gate`. **No `pub use` re-export** — smokes reach
`wolf_gnome::editor_inset::MAX_GRID_COLS` by module path and `drawer` needs no
shortcut.

### 3.2 `actions.rs`

- `ActionBinding` gains `pub ctrl_fold: Option<u8>` — the control byte this chord's
  folding consumes, **declared per row**. Every existing row declares `None`;
  `win.terminal` declares `Some(0x00)`.
- Extract the inline name→symbol map out of `key_name_matches` into
  `fn key_symbol(name) -> Option<char>`, widened to the folding names the grammar
  admits but the matcher does not know: `bracketleft`, `bracketright`, `backslash`,
  `asciicircum`, `underscore`, `at`, `question`, alongside `equal`, `minus`,
  `grave` and the single-alphanumeric branch. `key_name_matches` keeps its
  `Left`/`Right` early returns and otherwise reads `key_symbol`, so **a name that
  folds is by construction a name the matcher can match.**
- `pub enum CtrlFold { NotCtrl, NoFold, Byte(u8), UnknownKey }` and
  `pub fn ctrl_fold(&ParsedAccel) -> CtrlFold`. Symbol in `0x40..=0x7F` →
  `Byte(sym & 0x1F)`, with `?` → `Byte(0x7F)`. **`UnknownKey` rather than `NoFold`
  for an unresolvable name is what stops the checker being vacuous for exactly the
  rows it exists to catch.**
- `pub const CTRL_FOLD_FORBIDDEN: [u8; 2] = [0x1B, 0x7F]` — Escape and DEL, which
  no accelerator may consume **whatever it declares**.
- `forbidden_reason(p, declared: Option<u8>)` — keeps its two existing rules, then
  adds, in order: `UnknownKey` → refuse; fold in `CTRL_FOLD_FORBIDDEN` → refuse
  regardless of declaration (**this is the arm that makes `<Control>bracketleft`
  loud**); fold `!=` declared → refuse, with distinct messages for missing and for
  wrong. One checker, not two, so a caller cannot run half of it.
- `win.terminal`: `status: Bound`, `ctrl_fold: Some(0x00)`, note rewritten — the
  assumption is discharged by M18 and the price stated (the backtick spelling of
  `<C-@>`; `Ctrl+Shift+2` survives unbound).
- `MENU_SECTIONS`: add `{ label: "Terminal", action: "win.terminal" }` to the
  Back/Forward/Toggle-sidebar group. **The label must be the registry title
  `"Terminal"`** or `menu_labels_match_the_registry` reddens.

### 3.3 `command_action.rs`

`UiAction::ToggleTerminal` (doc-commented like the neighbouring `ToggleFullWidth`);
`Command::Terminal => UiAction::ToggleTerminal` replacing the `Deferred` arm whose
reason text this unit falsifies; module docs eighteen → nineteen.

### 3.4 `shell.rs`

`UiAction::ToggleTerminal => self.send_input(InputEvent::ToggleTerminal)`, beside
the `ToggleFullWidth` arm and for its stated reason (`WolfCore::toggle_terminal` is
itself a one-line `send_input` onto the same ordered channel, `api.rs:1034`).
Nothing else: `install_actions` already installs any row classifying to a real
`UiAction`, and `install_chord_controller` already matches through `action_for_key`
— R1's two switches are §3.2 and §3.3.

**The empty state deliberately does not disable `win.terminal`**: the captive
Neovim runs in a project-less window and a drawer there is a working shell, the
same argument `DECISIONS.md` records for the buffer picker.

### 3.5 `pane_view.rs` — the bulk

1. **State.** `PaneInner` gains `drawer: Cell<DrawerState>` (`Copy`, so `Cell` not
   `RefCell`, matching `active_pane`).
2. **Structure.** `rebuild_pane_area` keeps its focus sample, host-unparent loop
   (which already covers the drawer, an ordinary `panes` entry) and child-removal
   loop, then appends the drawer host **after** the tree widget:
   `pane_area` children become `[editor tree (vexpand)] [drawer strip (fixed)]`.
   New `apply_drawer_frame()` sets `vexpand(false)` and the content height from
   `drawer_split(...)` with `min_h = editor_inset::min_pane_size(..).1`.
   **Rejected, with the reason to record:** a `gtk::Paned` for the drawer boundary.
   It hands GTK the re-derivation free, but also hands the user a draggable handle
   whose position has nowhere to live (the drawer is not in the tree, so
   `set_divider_fraction` cannot take it) and which snaps back on the next
   `max-position` notify — a control that visibly refuses input, which
   `DECISIONS.md` already rules out for the scrollbar.
3. **Height re-application.** `apply_drawer_frame()` runs from `rebuild_pane_area`
   and from `on_render_wake` between `refresh_previews` and `sync_pane_grids`. A
   window resize converges in **two frames**; nothing under `cage` can resize the
   toplevel, so no smoke observes it — a `tests/README.md` unproven row.
4. **Intents.** `DrawerOpen { win } => self.apply_drawer_open(win)`. The comment
   block at `:920-929` — both D1 and D2 — is **deleted with the arm it describes**;
   the new doc points at `DECISIONS.md` and `crate::drawer` and asserts nothing
   about reachability.
5. **`apply_drawer_open(win)`** — ported from `PaneContainerView.swift:2097-2104`.
   Idempotent: return when the drawer is bound **or** a pane already follows `win`.
   Otherwise allocate an id, `state.bind(id)`, resolve the grid via `grid_for_win`
   (may be `None`; M6 says there is no transient to race), insert a `PaneWidgets`,
   `rebuild_pane_area()`, `set_active_pane(Some(id))`.
   **The guard must read `state.is_open()`, never `panes.contains_key`** — a
   self-healing guard would delete S3's falsifier (O17).
6. **`apply_split`** gains macOS's guard (`:2085`): a target that is the drawer
   returns. Without it `PaneTree::split` no-ops on a non-leaf while `apply_split`
   still inserts a `PaneWidgets` — an invisible pane bound to a real window.
7. **`apply_focus`** becomes: read `drawer` and the pane-area size, resolve
   `origin`, call `drawer::focus_target(...)`, map to a `win`, `focus_win`.
   **The tree borrow must be dropped before the core borrow**, per the file's rule.
8. **`detect_closes`** gains the drawer arm **ahead of** the loop (`:2157-2176`):
   remove it from `panes` and `state.clear()`, **without** `tree.remove` and
   **without** the last-leaf guard, then run the existing loop. Ordering matters —
   leaving the drawer in the list would let `leaves().len() <= 1` `break` before a
   genuinely closed editor pane collapsed.
   `clear()` here also drops `last_editor`: a deliberate simplification over macOS,
   costing one MRU hop and removing a second lifetime. Record it.
9. **`set_active_pane`** calls `state.record_active(pane, tree.leaves().contains(&pane))`
   before the CSS loop, so the MRU funnels through one place.
10. **`reset_after_restart`** clears the drawer (O11).
11. **`apply_layout_rebuild`** clears the drawer where it replaces `panes`
    wholesale (`:2027`).
12. **Untouched and correct by construction** — state in doc comments rather than
    re-derive later: `layout_snapshot` iterates `tree.leaves()`, so the drawer
    cannot reach `state.json` (O2); `refresh_pane_state`, `sync_pane_grids`,
    `pending_resize`, `on_pane_resize`, `queue_redraw` and `draw_pane` all iterate
    `panes`, so the drawer gets its status line, its `full_width_for_win`-driven
    cap, its own `resize_grid` (O5) and its paint (R6) **with no drawer branch**
    (O13).

### 3.6 `crates/wolf-core/src/redraw.rs`

`const TERMINAL_BUFFER_PREFIX: &str = "term://"` and
`fn is_terminal_buffer_name(&str) -> bool`. `apply_open_files` applies it to
**both** halves before `canon`: an `active` naming a terminal becomes `None`, and
terminal entries drop from `files`. Filtering before `canon` also avoids a
pointless `canonicalize` syscall on a scheme string.

**Considered and rejected:** filtering in `init.lua`'s `listed_files()` on
`buftype ~= ""`. More precise at the source and would also drop help and quickfix
buffers, but O12 places the fix in core; core additionally protects a front-end
running against a mirror an older `init.lua` produced, and is fast-tier testable.
Say so in the comment.

### 3.7 `flake.nix`

Comment-only: line 345's `GATE-REQUIRED` count forty-eight → **fifty-one**.
`flake.lock` does not move.

## 4. Test files

`drawer.rs`, `actions.rs`, `command_action.rs` and `redraw.rs` gain inline
`#[cfg(test)]` tables. `crates/wolf-core/tests/nvim_restart.rs` gains one
real-Neovim `#[test]` — **chosen deliberately over a new file**, since there is no
Bazel here to verify a new `rust_test` target and `nvim_restart` is already wired
with the staged nvim tree, `init.lua`, the `WOLF_*` env and `tags = ["no-sandbox"]`
(`crates/wolf-core/BUILD.bazel:1079-1088`). Its `size = "small"` headroom cannot be
measured here — a ledger row. `smoke_headless.rs` gains three smokes plus rows in
the three mechanical fixture guards (those guards build their tables from the
assertion constants, so the edit adds constants, never retypes a term).

## 5. Test plan

Expected after: 969 + **7** fast/core tests + **3** smokes = **979**, with
`smoke_headless` at **58**. Each agent states the number it observed rather than
repeating this arithmetic.

### 5.1 Measurements that must precede an assertion

| id | What | Why it cannot be assumed | When |
|---|---|---|---|
| **M23** | What `:edit term://…` does to the pinned nvim — an error, an empty buffer, or a second spawned shell — driven through `InputEvent::Open` against a real core. | Decides whether §5.3's doc says "the replay fails" or "a shell is spawned per crash". The test must assert against what was measured. | Pass 1, scratchpad, nothing committed. |
| **M24** | The drawer's live `winwidth`/`winheight`/`&lines` under cage at both font sizes. | Only for `MEASURED:` doc notes — every §5.4 assertion compares two live values or a production constant, so none depends on an invented number. | Pass 8. |
| **M25** | S1 page stability: five consecutive runs plus `grim` md5 of three. The drawer statusbar draws the raw `term://<cwd>//<pid>:…` (M21/O13) and both cwd and pid vary per run — the shape of the title flake `tests/README.md` records. | If the page is unstable the OCR leg is a latent flake, not a test. | Pass 8. **O16: narrow the OCR rect first.** |
| **M27** | The persisted leaf count with the drawer open (expected 1). | The `state.json` assertion is vacuous unless a layout persists at all — `persistable_layout` suppresses an unnamed lone leaf, so S1 must open a real fixture first. | Pass 8. |

### 5.2 Fast tier — pure seams, no display, no Neovim

| # | Test | Falsifying edit | Predicted failure | Isolated |
|---|---|---|---|---|
| 1 | `drawer_split_table` | Delete the `has_drawer` short-circuit | closed row: `drawer_height` 200.0, want 0.0, at h=600 | Yes |
| 2 | `bottom_most_leaf_table` | Flip `>` to `<` | stacked pair returns the top leaf | Yes |
| 3 | `focus_target_table` (six bridge rules) | Delete the `dir == Down` guard on the bridge arm | `editor, Up, no neighbour` returns the drawer, want `None` | Yes |
| 4 | `drawer_state_records_only_editor_leaves_and_clear_empties_both` | Make `clear()` reset only the MRU | `id()` is `Some(7)` after `clear()`, want `None` | Yes. **Proves the rule, not the wiring** — that is S3's job. |
| 5 | `ctrl_fold_table` | Change the mask `& 0x1F` → `& 0x7F` | `grave` returns `Byte(0x60)`, want `Byte(0x00)` | Yes |
| 6a | `no_accel_makes_a_neovim_key_unreachable` | Delete `ctrl_fold: Some(0x00)` from `win.terminal` | fails naming `win.terminal`/`<Control>grave`, missing-declaration reason | Yes |
| 6b | same, negative rows | Delete the `CTRL_FOLD_FORBIDDEN` arm | the `bracketleft` row is accepted — **the chord folding to Escape got through** | Yes |
| 7 | `bound_set_is_exactly_this_unit` | Revert `win.terminal` to `Reserved` | one-element diff, missing `"win.terminal"` | Yes |
| 8 | `action_for_key_table` (3 rows) | Change `key_symbol`'s `"grave"` arm | `Ctrl+grave` returns `None`, want `Some("win.terminal")`, **while test 7 stays green** | Yes under that edit |
| 9 | `ui_action_table` | Reclassify `Command::Terminal` as `Deferred` | `case 20 … returned Deferred(...)` | Yes, alone |
| 10 | `dispatched_commands_are_the_expected_set` | Same as 9 | one-element diff, missing `"terminal"` | **No** — reddens beside 9; disclose |
| 11 | `terminal_buffer_names_are_recognised` | Prefix → `"terminal://"` | `term:///tmp/x//123:/bin/bash` returns false | Yes |

**Dropped deliberately:** a `pane_layout::encode` test that a non-leaf content
entry is ignored — `encode` walks the layout, so the property holds by
construction and has no plausible falsifying source edit. O2's evidence is S1's
`state.json` leg instead, whose falsifier is the design O1 rejected.

### 5.3 Real-Neovim tier

**12 — `nvim_restart::open_files_omits_a_terminal_buffer`.** With a drawer open,
`open_files_for_tabpage` reports no `term://` entry and its `active` is not the
terminal, so `liveness_ui::reopen_order` cannot replay it. Doc comment states
M23's answer. Falsifier: delete the filter. Predicted red: the `TabFiles` M20
measured — `active: Some("term://…/bin/bash")` and a third `files` entry. Isolated.
Drive the drawer through `core.toggle_terminal()` (as `nav_history_live.rs:655`
does), and **poll `win_buffers` for an `is_terminal` entry before reading the
mirror** so the assertion does not race the emit.

### 5.4 Cage tier — three smokes, each justified

**S1 — `terminal_drawer_opens_and_is_not_persisted_under_cage`** (`SMOKE_FONT_PT`).
The only evidence the drawer reaches a pixel, and **its red run is already
measured** — M15/M16 captured today's build with the drawer open in Neovim and no
terminal strip on screen. Scenario: temp cwd + sidebar filler; `:e` a fixture (so a
layout is persistable, M27); `wtype -M ctrl -k grave -m ctrl`; settle; type
`echo <MARKER>` + Return; settle; `grim`; `cp` the live `state.json` to a frozen
copy (the preview smoke's mid-run freeze); `<C-\><C-n>`; `<C-w>k`; readback 1;
`<C-w>j`; readback 2; `:qa`.

| leg | Assertion | Falsifier | Predicted red |
|---|---|---|---|
| b | OCR contains the marker | restore `DrawerOpen { .. } => {}` | no marker; M15/M16 measured exactly this page |
| c | frozen `state.json` has exactly one leaf, the fixture's path; no leaf starts with `term://` | insert the drawer via `tree.split(..)` — the design O1 rejected | two leaves, the second `term://<cwd>//<pid>:…` — P2 realized |
| d | after `<C-w>k`, `&buftype` is not `terminal` | revert `apply_focus` to its pre-unit body | still `terminal`; M11 measured nvim declining the pure intent |
| e | after `<C-w>j`, `&buftype` is `terminal` | delete the `Down` bridge arm | empty — focus stayed in the editor |

Both readbacks are one `writefile(printf(...))` line, typed in normal mode from the
focused window, so **neither depends on OCR**.

**S2 — `terminal_drawer_defaults_to_full_width_under_cage`**
(`FULLWIDTH_SMOKE_FONT_PT`). **Cannot share S1's font**: `tests/README.md` records
that the column cap provably cannot bind at `SMOKE_FONT_PT`, so full width is
unobservable there; and OCR is unreliable that small, so this smoke **captures no
image at all** — the trade `column_cap_and_full_width_toggle_change_the_grid_under_cage`
already makes. One readback from inside the drawer:
`printf('%d %d %d %d', winwidth(0), winheight(0), winwidth(1), &lines)`.

| leg | Assertion | Falsifier | Predicted red |
|---|---|---|---|
| b | `winwidth(1) == editor_inset::MAX_GRID_COLS` — read from the production constant, never retyped | — (premise) | the regime is wrong and c means nothing |
| c | **Row E:** `winwidth(0) > MAX_GRID_COLS` | ignore `full_width_for_win` for the drawer in `refresh_pane_state` | drawer reports exactly `MAX_GRID_COLS` |
| d | **O5:** `winheight(0) < &lines / 3` | skip the drawer in `sync_pane_grids`/`pending_resize` | equals `&lines / 3` exactly — M9/M10's "nvim re-divides only unsized grids" |

The strict inequality in d **holds by construction**: the front-end's row count
subtracts the statusbar strip and both padding gutters from a third of the pane
area before dividing by cell height, so it is always at least one row short of
Neovim's.

**S3 — `terminal_drawer_reopens_after_a_neovim_restart_under_cage`**
(`SMOKE_FONT_PT`). **Cannot be folded** into the existing crash smoke: O11's hazard
needs a real crash and a real in-place restart with the drawer bound *before* the
kill, and folding it would change that smoke's pre-crash capture and its delicate
`winheight` equality. Scenario: `with_nvim_pid_helper`; open the drawer; kill the
captive nvim; wait for the restart; press the chord again; readback window count
and `&buftype`.

Falsifier: delete the drawer clear from `reset_after_restart`. Predicted red: one
window and an empty buftype — `apply_drawer_open`'s `is_open()` guard is still true
against the dead nvim's id, so **the drawer can never be opened again for the rest
of the session.** M19 makes this certain rather than hoped: `detect_closes` can
never fire for the drawer because grid 4 survives the restart.

### 5.5 Coverage summary

**Row E** is verified, not implemented — no front-end code, and S2 leg c is O4's
observation in the running application. **If leg c cannot pass without new code,
that is a finding to report, not a licence to widen the row.** **O10**'s checker is
covered by 5, 6a and 6b with two independent reds.

## 6. Dependencies

None. No new crate, feature flag or version bump. `Cargo.toml`, `Cargo.lock`,
`Cargo.Bazel.lock` and `flake.lock` untouched, so no `CARGO_BAZEL_REPIN`.

## 7. Lint

No `#[allow]` anticipated. `focus_target` takes six arguments, under clippy's
seven — **bundling the drawer id and MRU into `DrawerState` is what keeps it
there**, and that is a reason to keep the struct. `#[must_use]` on every new `pub
fn` returning a value. Convert `f64` heights through `pane_view.rs`'s existing
`px`/`cell_floor`, never a bare `as`. `forbidden_reason`'s new parameter changes a
`pub` signature, but both call sites are in `actions.rs`'s own test module.

## 8. Documentation ownership

R = rewrite in place, A = addition. One owner per fact.

| Document | Change | R/A |
|---|---|---|
| `wolf-gnome/src/README.md` | New `#### drawer.rs`: purpose, the four functions plus `DrawerState`, the two divergences from `DrawerLayout.swift`, the two-file merge, and the scale-invariance reason the seam takes the pane-area size. | A |
| `wolf-gnome/src/README.md` | `#### pane_view.rs` Decisions: the drawer's structural place (second child, outside the tree, no divider), the two-frame height convergence, the restart clear. Link, do not restate `DECISIONS.md`. | R |
| `wolf-gnome/src/README.md` | `#### actions.rs`: `ctrl_fold` and the new `forbidden_reason` signature; the reserved-row Gotchas entry gains the fold declaration. `#### command_action.rs`: the dispatched count. | R |
| `wolf-gnome/DECISIONS.md:258` | **The bullet "There is no terminal drawer…" is the gap this unit closes.** Replace with the drawer's durable constraints: outside `PaneTree` and why (`PaneContainerView.swift:225` made that choice *for* this front-end); absent from `state.json` and `set_divider_fraction`; no divider drag; sized from its own allocation because nvim re-divides only unsized grids. | R |
| `wolf-gnome/DECISIONS.md` accel section | The ctrl-folding rule; `<Control>grave`'s price as the same bargain the `Ctrl+Shift+<letter>` rule makes. **Plus O15's M22 clause fix** ("every key but one" → but five). | R |
| `wolf-gnome/DECISIONS.md` window structure / limitations | Pane area is `[editor tree][drawer strip]`. The drawer statusbar shows `bash` over the raw `term://…` at parity with macOS (M21), with prettifying recorded as a cross-platform follow-up. | R/A |
| `wolf-gnome/tests/README.md` | 48 → 51 smokes, `55 passed` → `58 passed`, sub-counts, and the three new scenarios. New unproven rows: drawer height re-application on resize; `clear()`'s rule unit-tested while only S3 covers the wiring; the drawer statusbar text reaches no assertion. | R/A |
| `wolf-core/src/README.md` | `#### redraw.rs` Decisions: the mirror drops terminal buffers from `files` and `active`, because a terminal buffer is not a file to reopen. | R |
| `feat-list.md:202`, `:203` | Tick both Linux boxes. | R |
| `feat-list.md:205` | The paragraph says the drawer is deferred and the chord reserved; both become false. Rewrite: what the GTK drawer is, the bound chord and its price, and that Row E rides the ordinary `full_width_for_win` path with no front-end special-casing on either platform. | R |
| `macos-followups.md` | Five rows: the `apply_open_files` filter (a shared-core change to the macOS mirror) plus `nvim_restart`'s unmeasurable `size` headroom; **B5**, the macOS drawer restart hole, phrased as inferred (O14); O13's `term://` statusbar string; `drawer.rs`'s divergences offered for adoption in the `cell_metrics_math.rs` row's shape; the `flake.nix` count amendment. | A |
| `architecture.md`, `command-lifecycle.md`, `build.md`, root `README.md` | **No change needed — checked.** | — |

## 9. What cannot be verified on this host

- **No Bazel.** No new `wolf-core/tests/*.rs` file is created precisely for this
  reason, so **no `BUILD.bazel` edit is needed and none is made.** Unverified:
  `nvim_restart`'s `size = "small"` (60 s) headroom with one more real-nvim case.
- **No macOS.** Unverifiable: that `bazel test //...` passes with the
  `apply_open_files` filter; that the macOS crash restore behaves better with
  terminal buffers gone; B5's restart hole; every Swift-side divergence the new
  seam records.
- **`crates/wolf-gnome` has no Bazel target**, so none of §3.1–3.5 or the smokes is
  visible to `bazel test //...` at all.
- **Load-bearing predictions**: S1's page stability under a live bash prompt
  (M25), the drawer's exact grid numbers (M24), the two-frame height convergence
  (**converted by nothing** — no smoke can resize under cage; an unproven row), and
  that `wtype -M ctrl -k grave -m ctrl` activates `win.terminal` rather than
  reaching Neovim (M18 measured delivery, M18a measured that a bound chord wins;
  **S1's red run converts the composition**).


