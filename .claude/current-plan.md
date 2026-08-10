# Work unit 24 — native tabs, part one: one window, N live tabs

Branch: `linux-feat/native-tabs` off `linux` (`c6ccb95`, clean and green:
**999 tests / 94 suites, 60 cage smokes**).

## Scope

Part one of the `Native tabs, one Neovim per project` row (`feat-list.md:118`).
**Persistence is deliberately frozen at one tab** — see O5. The row stays `[ ]`
until part two.

---

## Reconnaissance — measured on this host, take as fact

### M1. Baseline
`cargo test --workspace` exit 0, **999 passed / 0 failed / 1 ignored, 94 suites**;
`smoke_headless` 60 passed in 502.20 s. `wolf-core/tests/tabpages.rs` 7 passed,
`tabpage_cwd.rs` 1, `wolf-ffi/tests/tabpages.rs` 1.

### M2. `adw::TabView` / `TabBar` exist and work — built and run under `cage`
libadwaita **1.9.0**, GTK **4.22.4**; workspace pins `libadwaita 0.9.1` with **no
version feature** (`crates/wolf-gnome/Cargo.toml:41`). Probe output confirmed
`append`, `insert`, `close_page` + `close_page_finish` (**the veto point** — the
handler must answer it and return `Propagation::Stop`), `n_pages`, `nth_page`,
`page`, `page_position`, `selected_page`, `set_selected_page`,
`connect_selected_page_notify` (**fires only on a real change**),
`connect_page_attached`, `TabBar::new/set_view/set_autohide`.

### M3. Grid ids do NOT churn across a tabpage switch

Measured on nvim 0.12.2: G1 stayed `2`, G2 stayed `4` across a full round trip, and
`grid_for_win(1000)` came back `Some(2)` — the same id.

**`crates/wolf-core/tests/tabpages.rs:24-26` claims the opposite** ("A tabpage
switch churns GRID ids"), and is contradicted by its own test 7 at `:490-495`.
A documentation defect to fix.

### M4. A background grid is FROZEN, not gone — and `detect_closes` is safe

`snap.grid(G_background).is_some() == true` in both directions, content
byte-identical after typing into the other tab. `pane_view.rs:1147-1148` prunes on
`snap.grid(g).is_none()`, so **a background tab is not pruned**; a genuine close
still removes the grid. Dimensions do move (80x24 → 80x23).

**But `resolve_pending_grids` cannot bind in a background tab** — it uses
`snap.grid_for_win(p.win)` (`pane_view.rs:1203`), measured `None` there.

### M5. Unit 22's `may_snapshot` prediction is FALSIFIED — and the real hazard is worse

`win_buffers` holds **both** tabs' windows, because `init.lua:488`, `:560` and
`:2305` all sweep `nvim_list_wins()` — **global across tabpages**. So
`reported_windows` never drops and the gate never shuts.

**The defect sits one layer down.** `WolfCore::pane_status(grid)` joins grid →
`snap.windows` (`api.rs:1185-1194`), and `snap.windows` **is** current-tabpage-only:

```
FOREGROUND  pane_status(G1) = { filename: "alpha.txt", abs_path: ".../alpha.txt" }
BACKGROUND  pane_status(G1) = { filename: "[No Name]", display_path: "", abs_path: "" }
BACKGROUND  snap.buffers has W1 = true      <- the data IS there, keyed by WINDOW
BACKGROUND  live_cwd = the OTHER tab's cwd
```

**So a naive port that refreshes a background tab persists that tab's layout with
empty leaf paths — a well-formed `state.json` that has silently lost its files.**
`may_snapshot` cannot stop it. `live_cwd` is one cell per core (`api.rs:1166`)
following whichever tab is current; `cwd_for_tabpage(handle)` is the per-tab answer
that already exists.

### M6. A background `adw::TabPage`'s child is unmapped and never drawn

```
A (selected)   mapped=true  w=1280 h=626 draws=1
B (background) mapped=false w=0    h=0   draws=0 resizes=[]
```

A never-shown page reports `0x0`, which `pane_view::pending_resize` already refuses
(`:1451-1453`). **A frozen background grid can never reach a pixel, because GTK
never asks that DrawingArea to draw.**

### M7. `close_tabpage` on the LAST tabpage is a no-op

Liveness `Alive` before and after, `current_tabpage` unchanged, grids unchanged —
the `pcall` at `init.lua:1188` swallows E784. **The front-end must shut the core
down**, exactly macOS's `Project.closeTabpage` guard + `detach` → `shutdown`.
Closing a non-last tab is clean: the grid leaves `grids`, `win_buffers` drops, and
`cwd_for_tabpage` / `open_files_for_tabpage` both go `None`.

### M8. A second tabpage turns nvim's own tabline on

Nothing in the repo sets `showtabline` (zero hits tree-wide), so the default `1`
applies. Grid 1 gains a tabline row and the pane grid drops to `80x23`;
`wolf-gnome` never paints grid 1, so it is invisible but real, and `WinInfo`
`start_row`/`height` go stale. The front-end's own resize wins the row back.

### M9. A cage smoke CAN see a tab bar — the key asymmetry from unit 22

`cage` → adw window → `TabBar` over a two-page `TabView` → `grim` → the capture
shows **both tab titles** and only the selected page's body. A second *toplevel* is
invisible to `cage`; **a tab bar is inside the one toplevel**, so a tabs smoke can
assert on pixels where unit 22's window smoke had to assert on `state.json`.
(tesseract read one title and not the other at default psm — the sparse-page
calibration this file already documents.)

### R1. A tab is a sub-entity of ONE window

`WindowHandle`/`WindowInner` (`window.rs:228-241`) hold `{ id, window, state }`; the
coordinator's registry is `Vec<WindowHandle>` and it assembles `Vec<WindowState>`.
The schema already nests `TabState` inside `WindowState`. **The coordinator's
registry, id minting, `raise`, `palette_windows`, `close_commit` and `quit` are all
untouched by tabs.**

### R2. What moves per-tab, what stays per-window

**Per-tab**: the `pane_view::PaneHandle` (`window.rs:343`), the `tree_view` sidebar
(`:418`), and the `plan.core_cwd`/`empty`/`sidebar_visible` triple threaded into
`persistence::build` and `shell::build`.

**Per-window**: one `WolfCore` and one wake channel (`window.rs:310-332`) — N
tabpages sharing it is the point; the crash/stall overlay (`:390`), which is a
property of the core and which macOS also fans to **all** views while fanning the
render to `currentView` only; the header bar, `win.` actions, accelerators and the
command drain.

### R3. `wolf-gnome` calls NONE of the tabpage API today

Zero call sites for `create_tabpage` / `set_current_tabpage` / `close_tabpage` /
`current_tabpage` / `cwd_for_tabpage` / `open_files_for_tabpage`.

### R4. `restore_plans` reads `tabs.first()` and drops the rest

`state_seam.rs:145`. **`STATE_VERSION` does not move** — it is `1`, `tabs` is
already a `Vec` with no `serde(default)`, and a two-tab file is valid v1 that macOS
already writes and reads.

### R5. The `:tab*` lockout does not fight a GTK tab bar — it is what makes one possible

`init.lua:1337-1345`: Wolf owns all tabpage lifecycle, and a user-typed `:tabnew`
would create an orphan tabpage no view is bound to. The recorded residual gap
(obscure typed variants, `:1344`) becomes **user-visible on GTK for the first
time**: an escaped `:tabnew` makes a tabpage with no `adw::TabPage`.

### D1–D5. Documentation/code disagreements found

- **`architecture.md:147`** — "Neovim renders only its current tabpage, so `windows`
  and every per-window map describe that tabpage alone". **Measured false** for
  `win_buffers`, `buffers`, `full_width`, `previews`, which are global. True for
  `windows`, `grid_for_win`, `active_win`, `cursor`. **This is the sentence unit
  22's falsified prediction was derived from.**
- **`wolf-core/tests/tabpages.rs:24-26`** — M3.
- **`init.lua:439-441`** — says "EVERY window of the current tabpage"; `:488` is
  `nvim_list_wins()`, all tabpages.
- **`init.lua:1414-1417`** — says `files` are "the tabpage's windows"; `:1431-1443`
  uses the whole instance's `buflisted` set.
- **`api.rs:1370-1371`** — `open_files_for_tabpage` described as per-tabpage files;
  **measured identical across tabpages** (only `active` is per-tab).
- **`api.rs:971`** — `cwd_for_tabpage` is a snapshot read, not a blocking RPC.
- **`init.lua:1310`** — `wolf_tab_cwd` is dead code with no core caller
  (`runtime/nvim/README.md:137-139` already says so).

---

## Orchestrator decisions — binding

**O1. Three units, and this is the first.** The measured surface is wide — a new
widget layer, a lifecycle, a per-tab split of three handles, an N-tab restore path,
and a persistence hazard that produces a **well-formed wrong save** rather than a
crash. Bundling them means the first red could be any of five subsystems.

**Unit 24 (this one): one window, N live tabs, no persistence.**
**Unit 25: persistence** — N `TabState`s, N-tab restore, the fourth `close_commit`
arm, and M5's background-tab hazard. **Unit 26: polish** — the title following the
selected tab, the palette's Switch-to-Tab sibling, the escaped-`:tabnew` orphan.

**O2. The render wake drives the SELECTED tab only.** macOS's answer
(`Project.swift:188`), and it makes M5's three facts **unreachable rather than
handled**: a background tab's `pane_status` is never asked, so it never answers
`[No Name]`. **No core change is needed, and that is the point.** Every other tab
holds its last paint — which M6 makes free, since GTK never draws it.

**Every pane refresh must gate on `snap.current_tabpage == my_tabpage`** before
touching anything (macOS's `PaneContainerView.swift:1114`). Without it the first
post-switch wake refreshes the new tab against the **old** tab's `snap.windows` and
clears its statuses — M5's defect reached by a different door.

**O3. A new tab inherits the window's folder.** macOS does
(`AppDelegate.swift:435`), and `command_action.rs:67` **already records it** as New
Tab's behaviour. It keeps `tracked_cwd` coherent for every tab of a window and makes
the sidebar, picker and search rooting correct with no core change. `create_tabpage(Some(window_cwd))`.

**O4. `set showtabline=0` in `runtime/nvim/init.lua`, and record the macOS
obligation.** M8's artifact appears the moment a second tabpage exists, which is
this unit. Neither front-end paints grid 1 — both drive `ext_messages` — so the line
is correct for both and strictly removes a wasted row. It is a **shared file**, so
`macos-followups.md` gets the re-verification entry.

**O5. Persistence is FROZEN at one tab, explicitly.** `state_seam::window_subtree`
keeps writing the selected tab only; every other tab contributes nothing. This is
the one thing that fails **silently** (M5), so it gets its own unit with red-first
evidence — specifically a test that a background tab's saved leaf paths are
non-empty. **Saying this out loud in the code is part of the unit**, or the next
agent will "finish" it.

**O6. The tab boundary cuts below the overlays.** The pane view and the sidebar go
**inside** a page (M6 makes that free). The four transient overlays, the crash
overlay and the header bar **stay per-window** — the crash overlay is genuinely
per-core, and keeping them out avoids rebuilding `picker`/`palette`/`buffers`/
`search` construction. **Overlays close on a tab switch.**

**O7. The last tab closing IS the window closing.** M7 measured `close_tabpage` on
the last tabpage as a literal no-op, so the front-end must route it into
`on_window_closing` rather than hoping nvim does something.

**O8. D1–D5's documentation defects are fixed in this unit.** `architecture.md:147`
in particular is the sentence that produced unit 22's falsified prediction, and
leaving it would produce another.

**O9. Hazards carried forward, binding on every agent.** Never `git checkout --` /
`git restore` / `git stash` — all work is uncommitted. Never restore an
`include_str!`'d asset with `cp -p`; plain `cp` + `touch`, and re-run green before
believing a restore. **No constant before its measurement; no assertion without a
captured red.** Any pre-existing test that moves is a **finding to record, never a
threshold to retune**.

---

## The implementation plan

**Full plan at
`/tmp/claude-1000/-home-jplein-dev-jplein-wolf/7a1ee4a2-ca6e-4c0e-89bf-439b63de901c/scratchpad/plan24.md`
— every agent must read it in full.** It carries the widget surgery (§2), the
selection gate as a pure rule (§3), the close lifecycle (§4), the `NewTab`
promotion (§5), the file-by-file lists (§8), scheduled measurements M10–M14 (§9),
the test plan with every falsifier (§10), the agent order (§11), and lint (§15).

**Approved in full** except where these override it.

## Orchestrator decisions on the plan — binding

**O10. Caveat C1 is ACCEPTED — the sidebar stays PER-WINDOW, narrowing O6.** The
evidence is measured consequence, not preference: moving `tree_view` into the page
forces `adw::TabView` above the `gtk::Paned`, which widens the window-level
`gtk::Overlay` from the pane area (1029 px) to the full window (1280 px).
**`OVERLAY_ORIGIN_X_EXPECTED = 251.0` is that overlay's own measured left edge**,
and the crash smoke's button click is derived from it — the click moves ~125 px and
`crash_overlay_and_restart_button_recover_under_cage` goes red, as does the
picker/preview overlap band. Overruling would cost three re-measured constants and
two pointer smokes, all under O9.

Two further reasons hold independently: N tabs would run N `FileSearch` traversals
of **one** project for no user-visible difference (O3 gives every tab the window's
folder), and `settle_reroot` would re-root N sidebars per settle.

**The sidebar must move into the page when a later unit gives tabs independent
working directories.** Record that as a named follow-up in `DECISIONS.md`.

**O11. Caveat C2 is ACCEPTED and must be stated, not buried.** With autohide,
`close_outcome`'s `CloseWindow` arm is **unreachable from the shipped UI in part
one** — the bar is hidden at one page, and only its close button and middle-click
raise `close-page`. It ships implemented because it is the correct answer wherever
it *is* reached and because M7 makes the wrong answer a **silent no-op**, but it is
covered by its pure table alone. `DECISIONS.md`'s Present limitations says so.

**O12. The smoke must record that capture C is NOT F4's discriminator.** M4 measured
a background grid as frozen and byte-identical, so page 0 repaints its own content
whether or not nvim switched — capture C stays green under F4, and the **readback**
is what discriminates. Saying that in the doc comment is what stops a later reader
treating the capture as the proof.

**O13. Pass order consolidates §11's ten agents into nine**, keeping the plan's
compile-safety: §13 warns the tree does not build between its steps 3 and 6, so
**A4's promotion source and A5's widget work land in ONE pass**. Test-writers and
coders stay distinct actors; **no coder writes an assertion**. The full-suite
regression gate sits at the end, after the docs, so a doc edit cannot mask a code
regression. Baseline **999 / 94 / 60**; expected ≈ **1008 / 94 / 61**, with the
**suite count unchanged because no new `tests/*.rs` file is added**.

## Status

Reconnaissance complete. Decisions O1–O13 binding. Plan approved. Pass 1 next.
