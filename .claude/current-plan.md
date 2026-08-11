# Native tabs, part two: per-tab state persistence (`wolf-gnome`, Linux)

Branch: `linux-feat/tab-persistence`, cut from `linux`.

## The unit

- **Save**: a window persists every tab's `TabState`, in tab order, into
  `WindowState.tabs`.
- **Restore**: a launch rebuilds each saved window's tabs in saved order, with
  their layouts — not just the first.

`wolf_core::state`'s schema already carries both. **No schema change.**

---

## 1. Findings (architect, verified in-tree)

### 1.1 The part-one `[No Name]` claim is NOT accurate as written

**True:** `WolfCore::pane_status(grid)` joins `grid -> Snapshot::windows[grid].win
-> Snapshot::buffers[win]` (`crates/wolf-core/src/api.rs:1185-1198`) and does
answer `[No Name]` with an empty `abs_path` for a background tabpage's grid,
because `redraw.rs`'s `win_hide` does `snap.windows.remove(&grid)` while keeping
the grid (`crates/wolf-core/src/redraw.rs:1386-1403`, pinned by
`win_hide_removes_window_but_keeps_grid_on_tabpage_switch` in
`crates/wolf-core/tests/tabpages.rs:618`).

**False:** *the gather never calls `pane_status`.* `PaneInner::layout_snapshot`
reads the per-pane cache `p.status.borrow().abs_path`
(`crates/wolf-gnome/src/pane_view.rs:910-933`, read at `:924`) and issues no core
call — its own doc says so at `:890-891`. The only `pane_status` caller is
`refresh_pane_state` (`pane_view.rs:801-884`, call at `:865`), which runs only
from `on_render_wake`, which `tabs_ui::tab_wake` gates. So a background tab's
cache is **last-known-good**, not `[No Name]`. A naive registry loop today would
persist a *stale* answer, not an empty one.

Residual damage of an ungated background refresh is also smaller than stated:
`detect_closes` keys on the **grid** (`pane_view.rs:1137-1152`) and `win_hide`
keeps grids, so background panes are not torn down; `sync_pane_grids` is refused
by `pending_resize` at a `0x0` allocation (`tests/README.md:239`). What an
ungated refresh actually damages is the background tab's **statusbar text,
title and resize**.

**Verdict:** the implementation is the fact and the prose drifted. Fifteen
places overstate it and must be rewritten in place (§7). `tests/README.md:239`
additionally *predicts* this unit supplies the gate's captured red; under this
design that prediction becomes permanently false and the entry must be
rewritten rather than left as a promissory note.

### 1.2 The window-keyed maps are global across tabpages — one comment denies it

`architecture.md:149` states the contract correctly: `windows`, `grid_for_win`,
`active_win`, `cursor` describe the current tabpage alone; `win_buffers`,
`buffers`, `full_width`, `previews` are global, because
`runtime/nvim/init.lua` sweeps `nvim_list_wins()` (`:448-449`, `:498`, `:570`,
`:2318`). Both maps are wholesale-replaced per emit
(`crates/wolf-core/src/render.rs:767-780, 821-827`).

**Contradiction:** `crates/wolf-core/src/redraw.rs:1418-1425` asserts inside
`win_close` that "a window in a non-current tabpage is absent from both maps by
construction". That premise is wrong.

**Nothing in the tree measures this.** `tabpages.rs` covers
`windows`/`grids`/`tab_cwds`/`list_buffers` across a switch, never `win_buffers`
or `buffers`. **The whole design rests on this premise, so measuring it is work
item one (T0). If T0 is red, stop and report — §2.1 becomes the design.**

### 1.3 `PaneWidgets` already holds the join `Snapshot::windows` loses

`PaneWidgets.win: i64` set at construction (`pane_view.rs:236-238`, `:351`) from
the split intent or `rebuild_layout`'s returned handles (`:717-726`).
`refresh_pane_state` already resolves previews by `win` rather than grid
(`:826`). So a leaf's buffer path and preview flag are resolvable for a
non-current tabpage, with no RPC and no refresh.

### 1.4 Other facts confirmed

- `WolfCore::cwd_for_tabpage` is a lock-free snapshot read, authoritative for a
  non-current view (`api.rs:956-973`). Not needed here (§2.4).
- `rebuild_layout` is blocking and realizes into **the current tabpage's** single
  starting window (`api.rs:932-954`); `create_tabpage` is blocking and *enters*
  the new tabpage (`api.rs:906-930`, `init.lua:1117-1121`);
  `set_current_tabpage`/`close_tabpage` are fire-and-forget (`api.rs:997-1014`).
- A restored, never-selected tab has **no panes at all**: panes come from
  `ensure_initial_pane` on a wake the Hold gate never grants, so
  `snapshot_readiness()` answers `(0, n)` and `may_snapshot` refuses
  (`pane_view.rs:944-959`, `state_seam.rs:284-291`).
- `:tabnew`/`:tabnext`/`:tabclose` are locked out in the shared runtime (OD4,
  `init.lua:1347-1414`), so Neovim's current tabpage moves only because this
  front-end asked. That makes §4's launch sequence deterministic.
- `feat-list.md:242` says nine blocking calls where `DECISIONS.md:273-275` says
  ten. Pre-existing drift; fix while in the file.

---

## 2. The chosen approach

**Read each leaf's content from the snapshot's window-keyed maps, keyed by the
pane's own Neovim window handle; refuse the whole tab's gather if any leaf is
unresolvable; gather every tab; and seed a restored tab's cache from its restore
plan so a tab the user has never looked at cannot be erased.**

| Mechanism | Prevents |
| --- | --- |
| Win-keyed leaf resolution (`previews[win]`, then `win_buffers[win]`) | reading `[No Name]` for a background tab, and reading a stale cache for one |
| All-or-nothing refusal when a leaf's window is in neither map | the silent-loss shape: a well-formed layout whose leaves are `""` |
| The tab's own cache as the refusal fallback (existing rule, one level down) | a refused gate contributing nothing |
| The restore seed | the first gather after a launch erasing tabs the user has not selected |

The silent-loss shape is **unreachable**: `""` can only be written where
`win_buffers[win].path` is genuinely `None` (a real unnamed buffer), never where
a window is absent. No arity guard anywhere; suppression stays
`pane_layout::is_empty_leaf`'s content rule.

### Rejected alternatives

- **2.1 Cache the last-known-good subtree per tab.** Cheapest, and it does make
  the loss shape unreachable, but buys it with permanent staleness: `:tabdo
  edit`, a plugin, or `wolf_switch_buffer` into another tab's window persists an
  answer that was true minutes ago and nothing can tell the user. Its mechanism
  survives as the *fallback*, which is the honest place for it.
- **2.2 `open_files_for_tabpage` / `Snapshot::open_files`.** Per-tabpage and
  RPC-free (`render.rs:530-544`), but carries no topology and no per-window
  mapping, so it cannot fill a `PaneNode`'s leaves; `architecture.md:149` records
  that only its `active` field is genuinely per-tab.
- **2.3 A new `pane_status_for_win(win)` in `wolf-core`.** Tidier, but changes a
  crate shared with macOS for a Linux-only need, and the front-end already holds
  every input. A later cross-platform unit if macOS wants it.
- **2.4 Per-tab cwds via `cwd_for_tabpage`.** `DECISIONS.md:101` defers
  independent per-tab working directories to the unit that moves the sidebar into
  the page, and `tabs_ui::new_tab_cwd` / `state_seam::snapshot_cwd` are documented
  as having to agree. Every tab persists the window's cwd, as today.

---

## 3. Per-file changes

### 3.1 `src/pane_layout.rs` — pure seam (new rules)

- `pub enum WinContent { Preview(String), Buffer(String), Unknown }` — what one
  leaf's Neovim window resolves to. `Preview(source_path)` from
  `Snapshot::previews[win]`; `Buffer(path)` from `Snapshot::win_buffers[win]`,
  `""` for an unnamed buffer; `Unknown` for a window in neither map, or a leaf
  with no `PaneWidgets` at all.
- `pub fn resolve_leaves(leaves: &[(PaneId, WinContent)]) -> Option<HashMap<PaneId, LeafContent>>`
  — **pure**. `Preview(p)` -> `{path: p, preview: true}`; `Buffer(p)` ->
  `{path: p, preview: false}`; **any** `Unknown` collapses to `None`. Doc must
  state: an empty path is a *resolved* answer (a real unnamed buffer) and an
  absent window is *not*, because mapping absence to `""` is the exact shape
  that writes a well-formed `state.json` over the user's files.
- `pub enum LayoutGather { Ready(Option<PaneNode>), Refuse }` — one tab's gather
  answer. `Ready(None)` is `persistable_layout`'s content suppression; `Refuse`
  means "contribute the cache".

`encode`'s missing-id fallback stays (defensive, table-tested) but is no longer
reachable from the gather.

### 3.2 `src/pane_view.rs`

- New private `PaneInner::leaf_contents(&self) -> Option<HashMap<PaneId, LeafContent>>`:
  one `read_snapshot()` guard bound to a local, walk `tree.borrow().leaves()`,
  per id `panes.get(&id)` -> `p.win` -> `snap.previews.get(&win)` else
  `snap.win_buffers.get(&win)` -> `WinContent`; drop both borrows and the guard;
  then `pane_layout::resolve_leaves`. **No `p.grid` filter** — the grid is
  irrelevant to a win-keyed read, and a leaf whose grid has not resolved is now
  covered by the map lookup rather than silently dropped into an empty leaf.
- `PaneInner::layout_snapshot(&self) -> LayoutGather` — rewritten on
  `leaf_contents`; `None` -> `Refuse`, `Some(contents)` ->
  `Ready(state_seam::persistable_layout(pane_layout::encode(&tree.layout(), &contents)))`.
- `PaneHandle::layout_snapshot(&self) -> LayoutGather` — signature change.
- New `PaneHandle::realize_layout(&self, saved: &PaneNode)` — calls
  `apply_layout_rebuild(saved)` directly, no pending step, no one-pane
  precondition. Doc states the caller's obligation: **the tabpage this rebuilds
  into is whichever Neovim is current right now**, so the only legal caller is
  the restore materialization, immediately after `create_tabpage` returned.
- `request_layout_rebuild` / `consume_pending_layout` unchanged (tab 0's path).
- Module docs: rewrite the "Layout persistence / Save" bullet (`:51-77`).

### 3.3 `src/state_seam.rs` — pure seam

- `pub const MAX_RESTORE_TABS: usize = 8;` — restore policy, the
  `MAX_RESTORE_WINDOWS` precedent. Bounds the launch's **blocking-RPC budget**,
  whose worst case is the product of the two caps.
- `pub struct TabPlan { pub layout: Option<PaneNode> }` — a struct rather than a
  bare `Option<PaneNode>` so the deferred per-tab-cwd unit adds a field.
- `RestorePlan`: replace `layout: Option<PaneNode>` with `tabs: Vec<TabPlan>`.
  `sidebar_visible` stays **window-level**: the widget is per window
  (`window.rs:452-463`), the shell fans the preference to every tab
  (`shell.rs:901-909`), so every tab persists the same value and restore reads
  `tabs.first()`. State that asymmetry in the field's doc.
- `plan_for_window`: keep the cwd decision on `saved_tabs.first()`. Build `tabs`
  inside the `LaunchDecision::Folder` + `same_project` arm only —
  `saved_tabs.iter().take(MAX_RESTORE_TABS).map(|t| TabPlan { layout: t.layout.clone() })`
  — otherwise `vec![]`; then a **never-empty floor**: if empty, push
  `TabPlan { layout: None }`.
- `pub fn tab_subtree(cwd: String, sidebar_visible: bool, layout: Option<PaneNode>) -> TabState`.
- `pub fn window_subtree(width: i32, height: i32, tabs: Vec<TabState>) -> Option<WindowState>`
  — signature change. `x`/`y` stay `0.0`. **`None` for an empty tabs vec**,
  mirroring `snapshot_state(vec![])`: restore reads a `tabs: []` window as the
  empty state, so committing one destroys the project silently.
- `pub fn seeded_tab_state(restored_layout: Option<&PaneNode>, empty: bool, launch_cwd: &Path, sidebar_visible: bool) -> Option<TabState>`
  — `None` iff `restored_layout` is `None`; else
  `tab_subtree(snapshot_cwd(empty, launch_cwd, None), sidebar_visible, Some(layout.clone()))`.
  Doc carries both halves: seeding a tab that carries a layout stops the first
  gather after a relaunch erasing tabs nobody selected; **not** seeding one that
  carries none stops a launch whose project has gone committing an empty-state
  tab over the saved one before any pane bound.
- `may_snapshot` — **code unchanged**; rewrite the doc paragraph at `:269-278`.
- Module docs: rewrite the "**Tabs.**" bullet (`:18-22`).

### 3.4 `src/persistence.rs` — glue (one tab)

- `StateParts`: **drop `window`** (size read moves to `window.rs`); add
  `restored_layout: Option<PaneNode>`.
- `PersistInner`: `tab: RefCell<Option<TabState>>` (was
  `subtree: RefCell<Option<WindowState>>`), `restored_layout`, no `window`.
- `StateHandle::refresh_tab()` (was `refresh_subtree`): liveness ->
  `may_snapshot` -> `panes.layout_snapshot()`; `Refuse` returns leaving the cache
  untouched (the second refusal route); `Ready(layout)` -> `live_cwd` ->
  `snapshot_cwd` -> `tab_subtree` -> write cache. Borrow discipline unchanged.
- `StateHandle::cached_tab() -> Option<TabState>`: the cache when it exists;
  otherwise `state_seam::seeded_tab_state(...)`. Derived at read time rather than
  stored, so a sidebar toggle or an Open Folder reaches a tab whose gate never
  opened; the seed stops being consulted the instant a real gather succeeds.
- Module docs: `:11-26` now describes N tabs; the "refused gate contributes the
  CACHE" paragraph gains the seed and the second refusal route.

### 3.5 `src/tabs_ui.rs` — pure seam

- `pub enum RestorePress { Create, Wait, Idle }` and
  `pub fn restore_press(first_tabpage: i64, liveness: Option<Liveness>, pending: usize) -> RestorePress`.
  Total: `pending == 0` -> `Idle`; `first_tabpage == 0` -> `Wait`;
  `liveness != Some(Alive)` -> `Wait`; else `Create`. Doc carries all three
  reasons: creating before tab 0 has adopted leaves tab 0 to adopt the **last**
  created tabpage on a later wake (two pages, one tabpage —
  `TabParts::tabpage`'s recorded hazard); creating against a non-`Alive` core
  puts N blocking RPCs behind a Neovim already known not to answer; and answering
  anything but `Wait` on a refusal **loses the queued tabs outright**, because no
  cache holds them — their pages do not exist yet.
- Module docs: rewrite "## The selection gate makes a BACKGROUND tab's hazard
  unreachable" (`:13-30`). The gate still exists and is still the authority; what
  it protects is the background tab's **status cache, title and resize**, not
  `state.json`. Delete "no gate built on them helps; the defect sits one layer
  below them".
- `tab_wake`, `switch_target`, `close_outcome`, `new_tab_press`, `new_tab_cwd`,
  `tab_title` — unchanged. Rewrite the failure messages at `:594-597` and
  `:676-685` that say "once persistence follows the panes, that empty path is
  what gets saved".

### 3.6 `src/tab_view.rs` — glue

- `TabsHandle::open_tab(&self, parts: TabParts) -> bool` — **drop the
  `&adw::ApplicationWindow` parameter**. Pass `parts.layout.clone()` to both
  `request_layout_rebuild` and `StateParts::restored_layout`.
- `TabHandle::refresh_tab()` / `cached_tab() -> Option<TabState>` (renamed).
- `TabsInner`: add `pending_restore: RefCell<Option<RestoreTabs>>`, where
  `pub struct RestoreTabs { pub launch_cwd: PathBuf, pub empty: bool, pub sidebar_shown: bool, pub layouts: Vec<Option<PaneNode>> }`.
- `TabsHandle::queue_restore(&self, restore: RestoreTabs)` — called once by
  `window::build`.
- `TabsHandle::on_render_wake` — after the per-tab loop, call
  `self.inner.materialize_restore()`.
- `TabsInner::materialize_restore(&self)` — §4. Borrow discipline: the pending
  queue is `take`n in a statement of its own; `entries` is never borrowed across
  `open_tab`, `create_tabpage` or `set_selected_page` (the last fires
  `on_selected_changed` synchronously, which borrows `entries` and `core` and
  re-enters the shell).
- `reset_after_restart` — additionally clears `pending_restore`.
- Docs: rewrite `TabsHandle::selected`'s freeze paragraph (`:487-500`) — it is now
  the focus/overlay/empty-state answer and nothing to do with persistence;
  rewrite `TabInner::state` (`:118-120`) and `TabParts::layout` (`:104-107`).
  Module docs gain a short "## The restore materialization" section.

### 3.7 `src/window.rs` — glue

- `WindowHandle::refresh_subtree()` -> `for tab in entries() { tab.refresh_tab(); }`.
- `WindowHandle::cached_subtree()` -> collect `Vec<TabState>` from `entries()`
  via `cached_tab()` (`filter_map`), read `window.default_size()`, return
  `state_seam::window_subtree(w, h, tabs)`.
- `display_cwd()` unchanged.
- `build`: `plan.tabs.first()` supplies tab 0's layout; `open_tab` loses the
  window argument; when `plan.tabs.len() > 1`, `tabs.queue_restore(RestoreTabs { .. })`
  placed **before** the synchronous `shell.on_liveness_wake()` /
  `tabs.on_render_wake()` pair at `:682-684`.
- Module docs: rewrite `:76-83` and the blocking-RPC inventory `:28-48`;
  `refresh_subtree` / `cached_subtree` docs `:271-292`; `WindowInner::tabs`
  `:246-249`.

### 3.8 `src/shell.rs`

`new_tab`: `open_tab` loses the `&self.inner.window` argument. Nothing else.

### 3.9 `src/coordinator.rs`

Docs only ("## The state protocol", `:36-45`): a window's subtree is assembled
from its tabs' caches; a window with no tab contribution contributes nothing, so
"empty contribution set commits nothing" now holds at two levels.

### 3.10 `src/nav_ui.rs`, `src/lib.rs`

- `nav_ui.rs:17` — delete "`state_seam::window_subtree` is the one documented
  SINGLE-TAB site".
- `lib.rs:181`, `:239-252`, `:262` — rewrite; add `pane_layout::resolve_leaves`
  and `tabs_ui::restore_press` to the seam inventory.

---

## 4. The restore design

### 4.1 Sequence

1. `window::build` opens **tab 0** with `tabpage: 0` and `plan.tabs[0].layout`
   queued through `request_layout_rebuild` — unchanged — and queues
   `plan.tabs[1..]` on the tab layer.
2. On the first wake with a non-zero `Snapshot::current_tabpage`, `tab_wake`
   answers `Adopt(t1)` for tab 0; the entries loop refreshes it,
   `ensure_initial_pane` creates its pane, `consume_pending_layout` realizes its
   saved tree with one blocking `rebuild_layout` into t1. Today's path exactly.
3. Immediately afterwards, same wake, `materialize_restore` runs.
   `tabs_ui::restore_press(tab0.tabpage(), liveness, pending)`:
   - `Idle` -> nothing (steady-state cost: one `Option::is_none()` per wake).
   - `Wait` -> leave the queue; a later wake retries. No timer, no polling.
   - `Create` -> for each queued layout, in saved order:
     `core.create_tabpage(tabs_ui::new_tab_cwd(empty, launch_cwd, live_cwd))`
     (blocking; Neovim **enters** the new tabpage);
     `open_tab(TabParts { tabpage: handle, .. })`; then, while that tabpage is
     still current, `page.panes().realize_layout(&layout)` (blocking
     `rebuild_layout`). A `handle == 0` aborts the remainder with one message.
   - Finally `view.set_selected_page(page0)`, whose existing
     `on_selected_changed` issues `set_current_tabpage(t1)` fire-and-forget.
4. The queue is taken, so this runs exactly once per window per launch.

Realizing each tab's layout **inside** the loop is what makes the sequence
deterministic: `rebuild_layout` acts on the current tabpage, and the instant
after `create_tabpage` returns is the only moment this front-end knows for
certain which tabpage that is. **Deferring realization to first selection was
considered and rejected** — a deferred tab's files would not be open in Neovim
at all until selected, so `gettabinfo()`, `:bufdo` and the buffer picker would
disagree with the tab bar, and the cost would only move within the launch.

`open_tab` selects each new page, so materialization issues `T-1` transient
`set_current_tabpage` calls before the final one for t1. All are fire-and-forget
on the ordered channel and the last wins; keeping `open_tab` single-pathed is
worth more than avoiding them.

### 4.2 What the launch costs, exactly

Per restored window, `T = plan.tabs.len()` (capped at `MAX_RESTORE_TABS`):

- `T - 1` blocking `create_tabpage` RPCs.
- Up to `T` blocking `rebuild_layout` RPCs (one per tab carrying a layout).
- Zero additional: no `current_tabpage()`; tab 0's handle comes from the Adopt
  row, a snapshot read.

All launch-path, before the user interacts, and **none reachable from the restart
path** — `reset_after_restart` clears the queue. `DECISIONS.md` already accepts
one blocking `rebuild_layout` per window before the window is interactive; this
widens that to `2T - 1` bounded calls and must say so in "Crash and stall
recovery".

### 4.3 What a never-selected restored tab contributes

Its panes are never created, so `snapshot_readiness()` answers `(0, n)`,
`may_snapshot` refuses, and its cache stays `None`. `cached_tab()` therefore
answers the **seed**:
`tab_subtree(snapshot_cwd(empty, launch_cwd, None), <live sidebar preference>, Some(<restored layout>))`.

- The round trip is **idempotent**: launch, quit without touching the extra tabs,
  relaunch, and the same tabs come back with the same layouts. The seeded cwd
  equals the saved one by construction — a tab is seeded only when `same_project`
  held, which is exactly the equality of `plan.core_cwd` with the saved cwd.
- The seed follows live preference changes, being derived at read time.

A tab whose restored plan carried **no** layout takes no seed and contributes
nothing until its own gate opens — which preserves today's protection for a
launch whose project has gone.

---

## 5. Test plan (red-first)

### T0 — the premise, and it must run first

**File:** `crates/wolf-core/tests/tabpages.rs` (add a function; the file is
already a `rust_test` source in `crates/wolf-core/BUILD.bazel` — confirm, but
**no `BUILD.bazel` edit is expected**).

`per_window_buffers_are_global_across_tabpages`. Open `a.md` on T1; capture `w1`;
`create_tabpage(None)`; open `b.md`; poll until T2 is current. Assert from one
snapshot: `win_buffers` contains **both** `w1` (tail `a.md`) and T2's window
(`b.md`); `buffers[w1]` names `a.md`; and `windows` contains T2's window alone
(the control). Switch back to T1 and assert the mirror.

**Falsifier:** in `init.lua`'s `wolf_setup_buffer_tracking::emit_now`, replace
`nvim_list_wins()` with `nvim_tabpage_list_wins(0)`.

**If T0 is red on the pinned Neovim, stop and report** — §2 is unbuildable and
§2.1 becomes the design.

### Headless unit tests in `src/` (no wiring; this crate has no Bazel target by design, `DECISIONS.md:17`)

| # | File / test | Asserts | Falsifier |
| --- | --- | --- | --- |
| T1 | `pane_layout.rs` `resolve_leaves_table` | `Preview(p)` -> `{path: p, preview: true}`, `Buffer(p)` -> `{path: p, preview: false}`, over >=2 leaves | swap the two arms' `preview` flag; or route `Preview` through the buffer arm |
| T2 | `pane_layout.rs` `an_unresolved_leaf_refuses_the_whole_gather` — **keystone** | one `Unknown` among two resolved -> `None`; `Unknown` alone -> `None` | map `Unknown` to `LeafContent { path: String::new(), preview: false }` — the silent-loss shape; the message must say so |
| T3 | `pane_layout.rs` `an_unnamed_buffer_resolves_rather_than_refusing` | `Buffer(String::new())` -> `Some`, `path == ""` | treat an empty path as `Unknown` |
| T4 | `state_seam.rs` `restore_plans_covers_every_saved_tab_in_order` — **restore keystone** | 3 saved tabs, 3 distinct files -> `plans[0].tabs.len() == 3`, layouts in saved order | build `tabs` from `saved_tabs.first()` alone (part one's shape) |
| T5 | `state_seam.rs` `restore_plans_caps_a_hand_edited_tab_list` | `MAX_RESTORE_TABS + 3` -> exactly `MAX_RESTORE_TABS`, first-kept; at-cap not truncated | delete the `.take`; and separately tighten by one |
| T6 | `state_seam.rs` `a_window_always_plans_at_least_one_tab` | saved `tabs: []`, and the different-project case -> `tabs.len() == 1`, `layout: None` | delete the floor push |
| T7 | `state_seam.rs` `tabs_are_restored_only_into_the_project_they_were_saved_from` | 3 saved tabs, process cwd wins -> `tabs.len() == 1`, `layout: None` | build `tabs` above the `same_project` branch |
| T8 | `state_seam.rs` `seeded_tab_state_table` | `Some(layout)` -> `Some(TabState)` with that layout and `snapshot_cwd`'s answer; `None` -> `None`; an `empty: true` row pins the `""` cwd | return `None` unconditionally (restored tabs erased on first relaunch); return `Some` unconditionally (empty-state launch commits over a saved project) |
| T9 | `state_seam.rs` `window_subtree_writes_every_tab_in_order` (replaces `window_subtree_is_one_tab`) | 3 `TabState`s -> `tabs` in order, `x`/`y` `0.0`, size from args | `tabs.truncate(1)` |
| T10 | `state_seam.rs` `a_window_with_no_tab_contribution_commits_nothing` | `window_subtree(w, h, vec![])` -> `None` | return `Some(WindowState { tabs: vec![], .. })`; message cites the existing `saved tabs: []` plan row |
| T11 | `tabs_ui.rs` `restore_press_table` | total over `first_tabpage in {0, T_a}` x 4 `Option<Liveness>` x `pending in {0, 2}` (16 rows), plus the coverage sweep and non-vacuity guards the file's other tables use | (i) drop the `first_tabpage != 0` conjunct; (ii) drop the liveness conjunct; (iii) answer `Idle` where `Wait` belongs |
| T12 | `state_seam.rs` existing `restore_plans_table` / `plan_cases` | mechanical: every `want` gains `tabs: vec![TabPlan { layout: ... }]` | compatibility proof — must stay green throughout |

### Integration — the cage smoke

**File:** `crates/wolf-gnome/tests/smoke_headless.rs`.
`three_tabs_survive_a_relaunch_under_cage`, modelled on `RESTORE_LAYOUT_SCENARIO`
(`:6353`) and `single_pane_file_survives_relaunch_under_cage` (`:20225`).

**Three tabs, not two, and that is load-bearing.** After materialization Neovim
is current on the *last* created tabpage until the queued `set_current_tabpage(t1)`
lands, so the last tab may or may not get one refresh; only a *middle* tab is
guaranteed never to be Neovim-current at launch. Tab 1 is therefore the
deterministic never-selected tab whose seed assertion means something.

One `sh -c` inside one `cage`, sharing `WOLF_STATE_DIR` and one temp cwd:

- **Run 1:** `:e talfa.md`; `Ctrl+Shift+T`; `:e tbravo.md`; `Ctrl+Shift+T`;
  `:e tcharlie.md`; `:qa`; poll-and-reap (exit 11/12/13 as in the sibling). Copy
  `state.json` to `$TABS_STATE_COPY_1` before run 2 can write over it.
- **Run 2:** relaunch; settle `RELAUNCH_RESTORE_SETTLE`; write the readback with
  one `writefile(map(gettabinfo(), {_, t -> join(map(tabpagebuflist(t.tabnr), {_, b -> fnamemodify(bufname(b), ':t')}), ' ')}), ...)`
  — no double quote anywhere in the typed line; poll for it (exit 15). Then `:qa`
  **without ever selecting tab 1 or 2**, poll-and-reap (exit 16/17), and copy
  `state.json` to `$TABS_STATE_COPY_2`.

Assertions:

1. **Save side.** `$TABS_STATE_COPY_1` has `windows[0].tabs.len() == 3`, and
   `tabs[i].layout` is a single non-preview leaf named `talfa.md` / `tbravo.md` /
   `tcharlie.md` in that order. *Falsifier:* `window_subtree` truncating to one
   tab, or `cached_subtree` reading `tabs.selected()`. Predicted red: one tab,
   `tcharlie.md`.
2. **Restore side.** The readback is exactly three lines, `talfa.md` /
   `tbravo.md` / `tcharlie.md`. *Falsifier:* delete the `queue_restore` call in
   `window::build`. Predicted red: one line. Distinguishable from assertion 1's
   red, which is why run 1's file is frozen — the two-revert structure
   `single_pane_file_survives_relaunch_under_cage` documents.
3. **The seed.** `$TABS_STATE_COPY_2` still has three tabs with the same three
   layouts, in order. *Falsifier:* make `seeded_tab_state` return `None`
   unconditionally. Predicted red: two tabs (tab 0 gathered live, tab 2 gathered
   live after its one launch refresh, tab 1 gone) — the message must name that
   count, since it is the signature of the erase.

Conventions: fresh temp cwd; `CAGE_LOCK`; fixture names unlike the asserted terms
and registered in `RELAUNCH_FIXTURE_NAMES` (the mechanical guards
`restore_fixture_names_cannot_satisfy_their_own_assertions` and
`sidebar_filler_names_cannot_satisfy_any_smoke_assertion` will otherwise fail);
no `grim` capture and no OCR. Update the counts in `tests/README.md`'s
`smoke_headless.rs` entry.

### What stays unproven (write into `tests/README.md`)

- The `Wait` arm of `restore_press` (a wedged core at launch) is table-tested
  only; no smoke can arrange it.
- `resolve_leaves`' refusal reaches no smoke: arranging a leaf whose window has
  left both maps while its pane survives is timing-dependent under a compositor.
- The `MAX_RESTORE_TABS` cap, like `MAX_RESTORE_WINDOWS`, is table-tested only.

---

## 6. Implementation order

1. **T0** in `crates/wolf-core/tests/tabpages.rs`; run it; report before proceeding.
2. `pane_layout.rs`: `WinContent`, `resolve_leaves`, `LayoutGather` + T1–T3.
3. `state_seam.rs`: `MAX_RESTORE_TABS`, `TabPlan`, `RestorePlan.tabs`,
   `plan_for_window`, `tab_subtree`, `window_subtree`, `seeded_tab_state` +
   T4–T10, T12. Breaks `persistence.rs`, `window.rs` — expected.
4. `tabs_ui.rs`: `RestorePress`, `restore_press` + T11.
5. `pane_view.rs`: `leaf_contents`, `layout_snapshot -> LayoutGather`, `realize_layout`.
6. `persistence.rs`: `StateParts` without `window`, with `restored_layout`;
   `refresh_tab` / `cached_tab`.
7. `tab_view.rs`: `open_tab` signature, renames, `RestoreTabs`, `queue_restore`,
   `materialize_restore`, `reset_after_restart`.
8. `window.rs` and `shell.rs`: call sites; `refresh_subtree` / `cached_subtree`
   over the tab list; `queue_restore` at build.
9. `coordinator.rs`: docs only.
10. The cage smoke, red-first against step 8's build with each falsifier applied.
11. Documentation (§7), then `prettier --write` and
    `./tools/lint/markdown_check.sh` on every edited markdown file.

---

## 7. Documentation changes

The freeze is stated in **fifteen** places, counting source doc comments. Every
one is an edit in place, never an append.

**`crates/wolf-gnome/DECISIONS.md`**
- "Application state", gate paragraph (`:245`): the gate is read once per **tab**
  and asked about every tab.
- "Application state", bold freeze paragraph (`:247`): replace wholesale — a
  window persists every tab's subtree in tab order; a leaf's content is resolved
  from the window-keyed snapshot maps by the pane's own Neovim window handle,
  which are global across tabpages; an unresolvable leaf refuses the whole tab's
  gather and it contributes its cache; a restored tab contributes the layout it
  was restored with until its own first gather succeeds. State plainly that the
  `[No Name]` reading is a property of `pane_status`, which the gather no longer
  calls.
- "Application state", `sidebar_visible` (`:255`): the preference is per window,
  written into every tab, read back from the first.
- "Crash and stall recovery" (`:273-275`): the launch path now carries `2T-1`
  bounded blocking calls per restored window; `create_tabpage` has two triggers.
- "Present limitations": **delete** the "saved subtree carries its selected tab
  and no other" bullet (`:312`). Extend the restart bullet (`:315`) — a restart
  collapses to one tab and the next gather persists that collapse. Add one
  bullet: a tab whose restored plan carried no layout takes no seed.

**`crates/wolf-gnome/src/README.md`** — `tab_view.rs` (`:719`), `tabs_ui.rs`
(`:732` + `restore_press`), `state_seam.rs` (`:679-680` + `TabPlan` /
`MAX_RESTORE_TABS` / `seeded_tab_state`), `persistence.rs` (`:481-491`),
`window.rs` (`:808`), `pane_layout.rs` (`:455`, `resolve_leaves`' all-or-nothing
rule beside `is_empty_leaf`'s), `pane_view.rs` (`:468`). Watch the ~200-word
budget — several should shrink, not grow.

**Source doc comments** — `state_seam.rs:18-22`, `:269-278`, `:299-305`;
`tab_view.rs:104-107`, `:118-120`, `:487-500`; `tabs_ui.rs:13-30` and the two
test failure messages at `:594-597`, `:676-685`; `window.rs:28-48`, `:76-83`,
`:246-249`, `:271-292`; `persistence.rs:11-41`; `pane_view.rs:51-77`;
`nav_ui.rs:17`; `lib.rs:181`, `:239-252`, `:262`.

**`crates/wolf-gnome/tests/README.md`** — rewrite the tabs-gate entry (`:239`),
whose prediction that this unit supplies the gate's captured red is now false.
Add the new smoke and update counts. Add the three "unproven" entries.

**`feat-list.md`** — tick the Linux column of "Native tabs, one Neovim per
project" (`:118`); rewrite `:130-132`; update the `state` paragraph at `:138`;
drop item 1 from "Suggested order of work" (`:250`) and renumber; fix the stale
blocking-call count at `:242` (nine -> ten).

**`architecture.md`** — **no change**. `:149` and `:151` already state the
cross-crate contract correctly and at the right level.

**`crates/wolf-core/src/redraw.rs:1418-1425`** — a genuine divergence: "a window
in a non-current tabpage is absent from both maps by construction" contradicts
`architecture.md:149` and `init.lua:448-449`. Once T0 has measured it, correct
the premise in place. Keep the no-prune conclusion on its surviving ground
(Neovim never reuses window handles, so an orphan is bounded memory); do **not**
widen the edit into a behaviour change to `nav_history`.

---

## 8. Dependencies and lint

No new crate, no schema change, no `BUILD.bazel` edit expected.

- `#[must_use]` on `resolve_leaves`, `restore_press`, `tab_subtree`,
  `window_subtree`, `seeded_tab_state`, `PaneHandle::layout_snapshot`,
  `TabHandle::cached_tab`.
- `clippy::too_many_arguments` bites at seven; `RestoreTabs` / `TabParts` /
  `StateParts` keep constructors under it. `seeded_tab_state` takes four.
- `materialize_restore` stays factored (a `create_one_restored_tab` helper)
  rather than earning a `too_many_lines` allow.
- `window_subtree` consuming `Vec<TabState>` is deliberate.
- No `#[allow]` should be needed; adding one is a signal to restructure.

---

## 9. Findings from the implementation pass, and the orchestrator's rulings

### 9.1 §4.3's premise is FALSE — a restored tab is gathered live, never seeded

§4.3 claimed a never-selected restored tab "creates no pane, so
`snapshot_readiness()` answers `(0, n)`". Measured false: `realize_layout` ->
`apply_layout_rebuild` creates that tab's panes **synchronously** and resolves
their grids, so the tab has a bound pane the moment materialization returns.
From then on `refresh_tab` runs for it on every wake (the window refreshes every
entry, not the selected one), `may_snapshot` passes, and `leaf_contents`
resolves its file from the window-keyed maps while Neovim is on another tabpage.

**Consequence:** the smoke's assertion 3 did **not** redden under
`seeded_tab_state` returning `None`. The seed's surviving purpose is narrower —
a tab whose `realize_layout` fails (`decode` rejects the saved tree, or
`rebuild_layout` returns a mismatched handle count) — which is not arrangeable
under a compositor. **The seed is table-tested only (T8).**

**What assertion 3 actually discriminates**, measured: replacing `leaf_contents`'
`win_buffers[win]` lookup with the current-tabpage-only join `pane_status` does
produced a frozen run-1 file reading `0 None / 1 None / 2 tcharlie.md` — the two
background tabs lost their files, the current one kept its. That is §2's
keystone measured end to end under a real compositor, and it is the red the
smoke genuinely owns.

**Ruling:** take the coder's option (a). Rewrite assertion 3's `diagnosis` and
the smoke's `## What it asserts` item 3 to name the falsifier it has; record the
seed as table-tested only. §4.3 above and the `DECISIONS.md` limitation bullet
need the same correction.

### 9.2 Assertion 1's predicted red names the wrong file

Under `window_subtree` truncating, `truncate(1)` keeps the **first** entry, so
the frozen file names `talfa.md`. `tcharlie.md` is right only for the other
revert the message names, `cached_subtree` reading `tabs.selected()`. **Ruling:**
split the sentence into the two reverts with their two names.

### 9.3 `tab_wake_table`'s failure messages are now false

`tabs_ui.rs:654` still reads "once persistence follows the panes, that empty path
is what gets saved"; `:697-701` has a milder form. Persistence no longer follows
`pane_status`. §3.5 already asks for this. **Ruling:** rewrite both.

### 9.4 A durability gap, and it is wider than first reported — FIX IT

Between `window::build` returning and the wake on which materialization runs,
only tab 0 exists, so the coordinator can commit a **one-tab** tree over the
saved multi-tab one. The coder framed this as a sub-second SIGKILL risk. It is
wider:

- `window::build` calls `shell.on_render_wake()` at `:718`, before `present()`,
  and that first drain contributes and commits while `current_tabpage` is still
  `0` and tabs 1..N do not exist.
- Worse, `shell.on_render_wake()` runs **ungated by liveness**. A launch into a
  wedged or dead Neovim never materializes at all (`restore_press` answers
  `Wait` forever), yet tab 0 still contributes — its cache is refused, so
  `cached_tab()` falls back to its seed and answers `Some`. The window therefore
  commits `tabs: [one]` and the user's other tabs are gone from `state.json`,
  persistently rather than for a frame.

That is the silent-loss shape this whole unit exists to make unreachable, moved
from the leaf level up to the tab-count level. It must be closed.

**Ruling — the fix is (ii), contribute the queued tabs' seeds, not (i) suppress
the commit.** Suppression freezes the tree; contribution keeps it truthful, and
it reuses the seam already built. A queued-but-not-yet-created tab is the purest
instance of what `seeded_tab_state` is for, which also gives the seed the
reachable purpose §9.1 took away.

New pure seam in `state_seam.rs`:

```rust
#[must_use]
pub fn pending_tab_states(
    layouts: &[Option<PaneNode>],
    empty: bool,
    launch_cwd: &Path,
    sidebar_visible: bool,
) -> Vec<TabState>
```

Maps each queued layout through `seeded_tab_state`, dropping `None`s, preserving
order. `TabsHandle` exposes the queued layouts (and the `RestoreTabs`
window-level values) so `WindowHandle::cached_subtree` composes **live tab caches
followed by queued tabs' seeds, in tab order** — which is the order they will
materialize in.

**T13** (`state_seam.rs`, `pending_tab_states_table`): two queued layouts plus
one `None` -> two `TabState`s in order carrying those layouts and
`snapshot_cwd`'s answer; the empty slice -> `vec![]`; an `empty: true` row pins
the `""` cwd. **Falsifier:** return `vec![]` unconditionally — a window whose
restore has not yet run commits a one-tab tree over the saved session, which is
§9.4's bug restored. A second test asserts the composed window contribution: a
window with one live tab and two queued layouts contributes **three** tabs.

Not reachable by a smoke (the honest window is sub-second on a healthy launch);
record in `tests/README.md`'s unproven list beside the `Wait` arm.

### 9.5 `materialize_restore`'s abort path is untested

A `create_tabpage` answering `0` mid-loop aborts the remainder and leaves the
already-created tabs standing. No table and no smoke reaches it, and none
plausibly can. **Ruling:** record in `tests/README.md`'s unproven list, do not
manufacture a test for it.

### 9.6 Accepted without change

- The coder ran `cargo fmt --all`, which reformatted eight hunks inside the
  test-writer's `#[cfg(test)]` modules. Pure layout; no literal, assertion or
  expectation changed. Accepted.
- Deviation (a): `realize_layout` clears the pending layout before rebuilding,
  which prevents a **second** blocking `rebuild_layout` on the selection path
  that no inventory lists. Correct, and §3.2/§4 should say so.
- Deviation (b): the `handle == 0` abort reuses `tabs_ui::switch_target`.
  Correct — that is already the documented single place the rule lives.

---

## 10. Second-pass findings and rulings

§9.4's fix is landed and green (1029 tests). T13's red is captured, and the
cage smoke was **measured** to stay green under T13's falsifier — the healthy
launch materializes on the same wake that first commits, so no commit ever
observes a non-empty queue. T13 is table-only, and that is now a measurement
rather than a prediction.

### 10.1 `Idle` and `Wait` are behaviourally identical at the only call site — ACCEPT

`materialize_restore` matches `Idle | Wait => return`. T11's falsifier (iii)
reddens the table and changes no production behaviour. **Ruling: accept.** The
two answers differ semantically (nothing queued vs. queued-but-not-ready) and
the seam is total by design; manufacturing a behavioural difference would add a
panic path for no gain. Record it in `tests/README.md`'s unproven list as a
naming distinction, and leave the doc as the coder already corrected it.

### 10.2 The abort path drops the remaining tabs — FIX IT

Once `materialize_restore` takes the queue, a `create_one_restored_tab` that
aborts mid-loop leaves the remaining layouts with **neither a page nor a seed**,
so the next commit drops them. That is §9.4's shape narrowed to the abort, and
it is reachable: `create_tabpage` answers `0` on an RPC error or timeout, which
a slow Neovim at launch produces.

The coder declined to re-queue because a retained queue makes `restore_press`
answer `Create` on every later wake, putting blocking RPCs on every frame. That
reasoning is right; the answer is to separate the two jobs the queue does.

**Ruling:** on abort, **retain the un-created layouts as a seed source but stop
further creation attempts.** Split the queue's two roles — "layouts still owed a
page" (which seeds the contribution) from "creation may still be attempted"
(which `restore_press` gates). An abandoned restore keeps speaking for its tabs
in `state.json` forever and never issues another RPC. This is the unit's own bar:
a silent-loss shape made unreachable rather than handled.

### 10.3 A queued tab's sidebar preference is frozen at queue time — FIX IT

`RestoreTabs.sidebar_shown` is captured by `window::build`;
`AppShell::toggle_sidebar` fans the live preference only to `tabs.entries()`.
A toggle before materialization reaches neither the queued tabs' seeds nor the
materialized tabs' own `StateHandle`. Narrow — restore reads `tabs.first()`, and
tab 0 is live and correct — but visible once the user closes tab 0.

**Ruling: fix, cheaply.** `toggle_sidebar` also updates the pending queue's
`sidebar_shown`. The shell already holds `TabsHandle`.

### 10.4 Extract `window_contribution` as a pure seam — DO IT

Nothing today tests that `cached_subtree` composes live caches and queued seeds
**in that order**; T13 models the composition inside the test rather than
exercising the glue, so a glue bug (seeds prepended, or `pending_restore` read
and discarded) is invisible. **Ruling:** extract

```rust
#[must_use]
pub fn window_contribution(
    width: i32,
    height: i32,
    live: Vec<TabState>,
    pending: &[Option<PaneNode>],
    empty: bool,
    launch_cwd: &Path,
    sidebar_visible: bool,
) -> Option<WindowState>
```

and reduce `cached_subtree` to one call to it. That puts the ordering under a
table and matches the crate's stated posture: a rule left inside a widget closure
is a rule the fast suite cannot execute. `clippy::too_many_arguments` bites at
seven — this takes seven, so pass the window-level values as one struct if
clippy objects.

### 10.5 Stale test prose — FIX

`crates/wolf-gnome/src/tabs_ui.rs:1385-1389` (`restore_press_table`'s "FALSIFYING
EDIT 3" doc) still says a refusal answering `Idle` means "the first gather after
it writes the survivors back over the save". With `pending_tab_states` in place
the save survives for as long as the queue is held; only the *session* loses
those tabs.

### 10.6 Record in `tests/README.md`'s unproven list (technical-writer)

- `pending_tab_states` / `window_contribution`: table-only; the healthy launch
  materializes on the commit wake, measured.
- `materialize_restore`'s abort path (§9.5), including its new retention rule.
- `Idle` vs `Wait` is a naming distinction, not a behavioural one (§10.1).
- `TabsHandle::pending_restore` answering `None` after materialization is what
  stops the seeds double-counting a tab that now has a page; the cage smoke's
  three-tab count assertion is what guards it.

---

## 11. Review findings and rulings

Reviewer's verdict: **needs revision** on two items; nothing found that loses a
user's data. The silent-loss shape was traced independently at both the leaf
level and the tab-count level and confirmed unreachable by construction.

### 11.1 C1 — `crates/wolf-core/src/redraw.rs:1423-1425` — FIX

Still asserts "a window in a non-current tabpage is absent from both maps by
construction". T0 measured the opposite. It contradicts `architecture.md:149`
and a test in its own crate's suite, and it is the reasoning a future reaper
change would lean on. §7 named it; it was missed. Correct the premise in place,
keep the no-prune conclusion on its surviving ground (Neovim never reuses window
handles, so an orphan is bounded memory), do **not** widen into `nav_history`.
The technical-writer drafted a replacement and correctly declined to edit source;
use it as the starting point.

### 11.2 C2 — `entries` is insertion order, not page order — FIX

`tab_view.rs:313` and `:829` both document `entries` as "in page order". It is a
`Vec` pushed by `open_tab`; nothing reconciles it with `adw::TabView` page
positions, and `build` connects no `page-reordered` and does not narrow
`AdwTabView:shortcuts`, so both libadwaita reorder routes are live — dragging a
tab in the bar, and `Ctrl+Shift+Page_Up`/`Page_Down`.

This unit is what made order load-bearing, and it claims in `state_seam.rs`,
in `window_contribution`'s doc and in three test messages that tab order is the
user's own and is replayed. Insertion order falsifies that claim.

**Ruling: fix the code, not the claim.** Make the persisted order **page order**.
Do it in a pure seam so the rule stays in the fast suite, per the crate's stated
posture:

```rust
#[must_use]
pub fn ordered_tab_states(positioned: Vec<(i32, TabState)>) -> Vec<TabState>
```

Stable-sorts by page position and strips it. `cached_subtree` collects
`(view.page_position(page), cached_tab)` pairs, runs them through it, and passes
the result as `window_contribution`'s `live` — so `window_contribution` is
unchanged and the blast radius is one function plus one seam.

`TabsHandle::entries()` keeps insertion order (its other callers are
order-insensitive fan-outs); instead **correct its doc** to say insertion order,
and point at `ordered_tab_states` for the one caller that needs page order. That
is cheaper and more honest than sorting a shared accessor.

Note the second-order fact worth a sentence in `DECISIONS.md`: reordering GTK
pages does **not** reorder Neovim's tabpages, so page order and tabpage order
diverge after a drag. Persistence follows what the user sees, which is page
order.

### 11.3 C3, C4 and three more stale doc sites — FIX

- `window.rs:365` links `crate::persistence::StateHandle::refresh_subtree`,
  renamed to `refresh_tab`. A `rustdoc::broken_intra_doc_links` warning that
  neither clippy nor `cargo test` surfaces.
- `coordinator.rs:42` uses the banned changelog word "now" ("holds at TWO levels").
- `tab_view.rs:689-690` — `build`'s doc says `open_tab` must wait for the window
  because "the persistence glue reads the window's size". `StateParts` carries no
  window; the size read moved to `cached_subtree`.
- `window.rs:243-244` — `WindowParts::plan`'s doc says "saved layout" singular;
  `RestorePlan` carries `tabs: Vec<TabPlan>`, one layout per tab.

### 11.4 T1 and T2 — smoke evidence and a false-diagnosis risk — FIX

- **T1**: the smoke's doc records "Predicted red" for assertions 1 and 2 while
  assertion 3 says "MEASURED red" and quotes the file. Both reds **were**
  captured. Land that evidence in the doc comment, in the measured-table shape
  `two_windows_two_neovims_and_two_saved_sessions_under_cage` uses.
- **T2**: run 2 gates on a fixed settle and `wait_file` waits for the file, not
  for three lines. Materialization needs a notification plus four blocking RPCs,
  so a slow host produces one or two lines and assertion 2 fails claiming
  `queue_restore` was deleted — a **false diagnosis**. Poll for three lines,
  re-issuing the `writefile` in the loop, with its own exit code, matching run 1's
  exit-21 shape.

### 11.5 Accepted without change

- **C5**: `cached_subtree`'s `Path::new("")` fillers are safe because the same
  `map_or` supplies the empty slice that makes them unread, and the coupling is
  commented where a reader needs it. No action.
- The two `#[allow(clippy::float_cmp)]` in `state_seam.rs` match a pre-existing
  sibling and fire for real.
- `architecture.md` unchanged — verified independently by both the
  technical-writer and the reviewer.

---

## 12. Final rulings

### 12.1 Reorder reachability, measured

Zero matches for `set_shortcuts` / `page-reordered` / `connect_page_reordered`.
`adw::TabView::set_shortcuts` is `#[cfg(feature = "v1_2")]` and this crate builds
`libadwaita` with `default` alone, so it **cannot** narrow the property. The
`Adw-1.gir` default includes both reorder shortcuts.

**The drag route is unconditionally reachable** — `AdwTabBar` reorders by drag
with no property that disables it. That alone makes C2's fix load-bearing rather
than defensive.

**The keyboard route is NOT confirmed.** `window::wire_keys` adds a bubble-phase
key controller on the toplevel and `input::named_key` maps `Page_Up`/`Page_Down`
regardless of modifiers, returning `Propagation::Stop` — so wolf's own controller
very likely swallows `Ctrl+Shift+Page_Down` and forwards it to Neovim before
`AdwTabView`'s managed-scope shortcut controller runs. Not measured.

**Ruling:** the source docs are already narrowed to what was measured. Narrow the
**test** prose the same way — `state_seam.rs:3137` says "Both of libadwaita's
reorder routes are live" and `:3232` names `Ctrl+Shift+Page_Up` as a
user-reachable action. Claim the drag route, which is measured; do not claim the
chord.

### 12.2 The glue-wiring gap — RECORD, do not build a smoke

`ordered_tab_states_table` executes the seam, but nothing executes
`cached_subtree`'s pairing. A glue bug pairing each cache with its `enumerate()`
index instead of `tab.page_position()` would satisfy every headless row and
reintroduce C2. The existing three-tab smoke does not discriminate it, because in
an un-reordered window position and index coincide.

The coder proposed `a_dragged_tab_is_persisted_in_its_new_position_under_cage`.
**Ruling: record it in `tests/README.md`'s unproven list instead.** The drag
smoke needs a new capability (dragging a bar entry under `cage`) to guard a
two-line glue bug, on a unit that has already grown well past its plan. The repo
has a documented convention for exactly this class of gap, and using it honestly
is better than a capability built in a hurry at the end of a long unit. State the
gap precisely, including the `enumerate()` bug it would catch, so the next unit
that touches the tab bar knows what it owes.

### 12.3 `src/README.md` word budget

Eight advisory findings. Five entries were already over before this unit
(`pane_view.rs` 255, `shell.rs` 258, `tabs_ui.rs` 253, `window.rs` 261, plus the
technical-writer's own `smoke_headless.rs` 252 and `tabpages.rs` 256). Two are
newly over because of this unit: `state_seam.rs` 263 and `tab_view.rs` 271.

**Ruling:** rebalance the two this unit pushed over, and `window.rs`, which this
unit added to. Do not delete a rule that carries a reason —
`CLAUDE.md` says to expect a net-negative pass, so cut restatement and prose that
30 seconds of reading the source would answer. Leave the entries this unit did
not touch alone; they are a pre-existing debt and not this unit's to pay.

---

## Status

- [x] Problem statement
- [x] Architecture
- [x] **T0 measured — the premise is TRUE.** While T2 is current, `win_buffers`
      holds keys `{w1, w2}` (both tabpages' windows) and `buffers[w1]` still
      names the canonical `a.md`; `windows` held `{w2}` alone. The mirror after
      switching back holds too. Red captured under the `nvim_tabpage_list_wins(0)`
      falsifier at `init.lua:498` (assertion 1, the discriminator, reporting a
      live key set of `{1001}`); revert byte-identical. `tabpages` suite 10/10,
      clippy clean. §2 is buildable; §2.1 does not become the design.
      **Corollary:** `crates/wolf-core/src/redraw.rs:1418-1425` is now
      measured-false and must be corrected per §7.
- [x] Tests (red-first) — T0–T13, `ordered_tab_states_table`, `window_contribution_table`,
      the two abandoned-restore tests, and `three_tabs_survive_a_relaunch_under_cage`
- [x] Implementation — §3, §4, §9.4, §10.2/10.3/10.4, §11.1/11.2/11.3, §12.1
- [x] Review — verdict "needs revision" on C1 and C2; both fixed, plus C3/C4 and
      three broken intra-doc links `cargo doc` surfaced. No data-loss defect found;
      the silent-loss shape traced independently and confirmed unreachable at both
      the leaf level and the tab-count level.
- [x] Docs — `DECISIONS.md`, `src/README.md`, both `tests/README.md`, `feat-list.md`,
      and `redraw.rs`'s measured-false comment. `architecture.md` unchanged, verified
      twice independently.
- [x] feat-list tick
- [ ] merge to `linux`

## Final state

Gate green on Linux/NixOS: `cargo fmt --check`, `cargo clippy --workspace
--all-targets -D warnings`, `cargo test --workspace` = **1033 passed, 0 failed,
1 ignored** (the pre-existing PNG-golden dev aid). `cargo doc` clean of
unresolved intra-doc links in `wolf-gnome`. `markdown_check.sh`: five
entry-budget findings, all pre-existing, plus three advisory size warnings.

**Not exercised on this host:** Bazel and the macOS front-end. No `BUILD.bazel`,
dependency, feature or asset change was made, and the one Rust test file touched
outside `wolf-gnome` (`crates/wolf-core/tests/tabpages.rs`) is already an
enumerated `rust_test` source, so no wiring gap exists — but `bazelisk test
//crates/wolf-core:tabpages` is owed on the next macOS pass.
