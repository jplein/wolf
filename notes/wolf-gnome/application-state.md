# Application state

Last verified at commit `7e86d343567b46e1309968bb6c18a865794fed7b`.

The saved state is the `window_state` and `project_state` rows of `wolf.sqlite`, written through the one `wolf_core::AppStateStore` in the model [application-state.md](../../specs/application-state.md) defines. The model is shared with the macOS front-end, so a Linux-only change to it is a macOS re-verification obligation. `wolf_core::state_dir()` is the single resolution point, and `WOLF_STATE_DIR` overrides it.

Each window re-gathers every one of its tabs on every render wake of any of them, with no timer and no debounce, and once more as the first thing its close handler does; the coordinator assembles the window state from every window and submits it. A wake refreshes only the waking window and reads the others out of their caches, so the cost does not grow with idle neighbours.

That is affordable because two coalescers already sit beneath it: the store is a no-op on an unchanged tree, and the writer thread coalesces before it writes. The crate's no-polling property survives, and a kill loses at most one coalesce window.

**Every gather is gated, and the gate prevents data loss rather than churn.** A tab gathered while Neovim is dead, before the first pane binds a grid, or before Neovim has reported its per-window buffer map would persist an empty layout and overwrite the user's saved one with nothing. The gate is read once per tab, against that tab's own core, and it is asked about every tab.

**A window persists every tab, in tab order, and a background tab's entry is as trustworthy as the selected one's.** Each tab is gathered from its own Neovim's snapshot: each pane's content is resolved from `Snapshot::previews` and `Snapshot::win_buffers` by the pane's own Neovim window handle, so the gather reads live values with no RPC and never calls `pane_status`. A pane whose window is in neither map refuses that tab's whole gather, and the tab contributes its cache.

**Tab order is page order — the order the bar draws — and never the order the tabs were opened in.** `AdwTabBar` lets the user rearrange the bar by dragging a tab, with no property that turns that off, and the restore path replays a saved window's tabs positionally, so persisting the order pages were created in would discard that arrangement at the next launch and then write the discard back.

**A restored tab contributes its saved tab until its own first gather succeeds**, as saved and not as `app_state::restored_tab` rewrote it, with only its sidebar following the live one; for a tab whose layout binds no pane, that is forever. So a launch that finds a file missing does not write the loss back, and a tab restored empty because its directory vanished keeps the saved tab until its own empty-tab gather replaces it, never touching that directory's project state by absence. A tab the user opened has no seed and contributes nothing until its gate opens.

**Each tab restores at its own directory.** Each saved tab is planned through `workspace::launch_decision` with its `directory` alone: a usable one restores the tab there, and a missing one, `""`, `$HOME` or `/` restores an empty tab whose Neovim starts where the decision says. Each tab persists its own Neovim's working directory, so a tab moved with `:tcd` comes back where it was.

**A terminal launch shows its directory without dropping a saved window.** When the process cwd is a usable project that no saved tab holds, `restore_plans` puts one fresh window there in front of the saved windows, which all restore; when a saved tab holds it, the session restores exactly as saved. That keeps the Linux idiom of a terminal launch showing its directory.

**A folder tab always writes its root, a lone empty edit pane included.** A tab's open buffer paths have no persistence channel of their own — they ride only inside its root's panes — so a save side that skipped a lone pane would silently stop persisting the single most common shape there is: one window, one tab, one pane, one file. Only an empty tab writes `root: null`, as [§2](../../specs/application-state.md#tab) allows.

**The restore side skips a rebuild by content, never by arity.** It skips only a root `pane_layout::is_empty_leaf` matches, a lone empty edit pane, which a fresh tab already shows, and rebuilds any other root with no arity guard at all. A skipped tab still takes its saved focus, zoom and drawer. That is the shape of a bug already fixed on macOS; do not reintroduce it here.

**Window position is deliberately never persisted, permanently.** A Wayland client is given neither its own position nor screen geometry, so there is no analogue of the macOS check that a restored frame still intersects a screen — a restored coordinate could not be validated even in principle. Only the size is restored, and `x` and `y` are written as `null` rather than a plausible-looking value a later reader might trust.

**Nothing about the content-width toggle is persisted, deliberately and permanently.** `b:wolf_full_width` is a buffer variable Neovim owns and re-emits on every `BufEnter` and `WinEnter`, and the model has no field for it — adding one would change a model shared with macOS for a Linux-only feature. It would also key by pane path what Neovim keys by buffer, so one file open in two panes would get two persisted answers and one live one. The honest consequence is that the toggle does not survive a quit and relaunch, which is parity: macOS persists nothing either.

**The sidebar preference and the empty state are each tab's own.** `sidebar.visible` carries the tab's preference and never what is on screen: the empty state hides the sidebar without changing what the user asked for, so persisting the visible flag would make one project-less session hide the sidebar of every later one. `state_seam::sidebar_is_shown` combines the two at apply time, and a restored `false` is always reversible — from the header bar's toggle group or from `Ctrl+Shift+B`.

The window's one sidebar, its toggle group and the `win.` actions the empty state disables follow the selected tab, re-applied on every real selection change. A toggle changes the selected tab's preference alone. The sidebar's root stays per window and follows the selected tab through the one settle point.

An empty selected tab whose live cwd is still its own canonical launch cwd neither re-roots nor leaves the empty state, or selecting it beside a folder tab would un-empty it. A typed `:cd` moves its cwd and un-empties that tab alone, even into the folder the sidebar is already rooted at; Open Folder replaces the empty tab with a folder tab instead ([why](the-coordinator-and-its-windows.md#opening-a-folder)).

An empty tab and its window are titled "Wolf", `title::WINDOW_TITLE_FALLBACK`, never by its Neovim's cwd: that is home, which is no folder. While the empty-state page shows, `TabHandle::refresh` reads no current file, since every Neovim names its anonymous buffer `Untitled.md`; a file opened into the tab from outside hides the page and titles the tab.

**The stacking order is kept from events, because Wayland lets a client query neither which window is in front nor which is minimized.** The coordinator's `stacking::StackingOrder` moves a window to the front when its `is-active` turns true. GTK 4 never sets `GDK_TOPLEVEL_STATE_MINIMIZED` on Wayland, and `xdg_toplevel`'s `suspended` state also means occluded or on another workspace (GTK merge request 6013), so only Wolf's own Minimize marks a window minimized, and any focus unmarks it. A window not yet ranked, such as a New Window before it gains focus, ranks after every ranked window of its class, in registry order.

A focus or a minimize during a quit updates the order and commits nothing, since the quit path has already committed the whole session. Restore seeds the order from the plans, frontmost first with each window's minimized flag, once every window is built and before any is presented. On a compositor that focuses each window as it maps, those focuses then re-rank the windows in map order, and the restore's present chain makes that order the saved one ([The coordinator and its windows](the-coordinator-and-its-windows.md)).

Each window's tab recency lives in `tab_view`'s `TabsInner`: a real selection change moves the tab to the front, a closed tab is forgotten, and a restore reseeds it with the active tab first and the rest in page order, since opening each planned tab selects it in turn. A window's `active_tab` is the selected tab's index among the tabs that contributed, or, when the selected tab contributed nothing, the most recently selected contributing tab's.

## New Tab

**New Tab captures what the sidebar shows, not the preference.** It opens New Window's empty tab, planned by the same `restore_plans` call so the two cannot diverge, and takes one value from the selected tab: `state_seam::sidebar_is_shown` of its preference and emptiness, so a New Tab from an empty tab captures hidden. The value is taken at New Tab, because the origin may close or change before a folder opens into the new tab, and it is kept in that tab's `TabInner`, in memory only. `open_folder::plan` applies it only to a fill whose folder has no project state; a stored tab keeps its own setting.

A typed `:cd` out of that empty tab is not Open Folder and ignores the captured value: the tab shows the sidebar by its own preference, which New Window's plan sets to shown.

## The extras

A folder tab also writes [§2](../../specs/application-state.md#tab)'s extras: its `focused` pane, its sidebar's page, query and toggles, and its drawer, open or closed and always with its share, plus each edit pane's `cursor` and `top_line` and each preview's `scroll`. `focused` and `zoomed` are written through `pane_layout::pane_ref`, as a tree leaf's depth-first index or `"drawer"` for the bound drawer, so a zoomed or focused drawer is saved as one. An empty tab writes its whole sidebar and no root, zoom, focus or drawer, since nothing it holds names a pane.

**Each tab keeps its own drawer share**, and a relaunch restores it through `DrawerShare::restored`, which refuses a share that is not finite and strictly between 0 and 1. How the divider follows it is [Window structure](window-structure.md)'s.

**The sidebar's page and the search panel's fields are recorded per tab, while the widgets stay per window.** The selected tab's record follows the widgets: the shell records it before every gather of the window and into the tab a switch leaves. A new tab records the Files page, an empty query and the initial toggles, as New Window's does. A switch shows the selected tab's recorded page and leaves the panel's query, toggles and rows alone, since the panel is per window ([Window structure](window-structure.md)).

At launch, and when Open Folder fills a tab, the panel takes the selected tab's record, and runs its search only on the Search page, in a tab with a project, for a query that is not blank. An empty tab's search would be rooted at its Neovim's home directory. A background tab's saved query and toggles therefore give way to the panel's once that tab is selected, which is the price of one panel per window.

## Capture moments

**Cursor, top line and scroll are captured at [§4's moments](../../specs/application-state.md#cursor-and-scroll-capture) only, never on every wake.** Each pane view keeps the last captured view of each tree leaf, and the gather writes it, so a save carries the view of the last capture. An edit pane's read is the blocking `WolfCore::window_cursor`, issued only while `state_seam::may_capture_cursor` holds, `Alive` alone, and a failed read keeps the last view.

- **A focus change.** `set_active_pane`, which every route into a pane goes through, captures the pane it leaves.
- **A tab switched away from.** The selection callback names the tab it left, which is captured after its sidebar record. Tab 0's page is selected before the tab is registered, so `open_tab` records that tab as shown itself, or the first switch away from it would name no tab.
- **A window closing.** Only the `KeepSubtree` arm captures; the other two write nothing of the window that survives.
- **Quit.** Every window is captured after `shutting_down` latches and before any Neovim exits.

The restart path never captures: `reset_after_restart` sets the active pane directly and clears the views, since a dead Neovim's windows are gone. A closed pane drops its view.

**A preview's scroll is pushed by its page, never read.** `evaluate_javascript` is asynchronous, so a quit could not wait for a read. Each preview's `WebView` carries a user script that posts the scroll fraction to a script message handler at load and on scrolling, at most once per animation frame, and the pane keeps the last report `preview_ui::reported_scroll` accepts; a capture copies it.

## Restoring the extras

`tab_view::open_tab` hands every planned tab that is not empty and has a saved tab to its pane view, which applies the extras after the rebuild, or with no rebuild for an empty root. A handle-count mismatch keeps the fresh pane and applies none. The restored views seed the views cache, so a pane no capture reaches before the next save keeps its saved view.

**A zoom wins the focus** (`pane_zoom::restore_targets`): focusing a pane the zoom hides would unzoom it on the next focus move. A saved `"drawer"` maps to the drawer only when the drawer restores open.

**A saved open drawer reopens with a fresh shell, and its zoom and focus wait for its bind.** The restore sends `toggle_terminal`, then a `focus_win` of a focused leaf, both on the ordered input channel, and the shell starts in the tab's Neovim's directory. At the drawer's bind a drawer zoom applies at once, since Neovim is on the drawer. A pane zoom waits for the wake on which Neovim's current window is that pane, since a wake that still reads the drawer is a focus move, which unzooms. Readiness waits for the bind.

**A restored edit view is applied a second time, once the tab's shown grids have settled.** The rebuild applies it at Neovim's own window size, and sizing each grid afterwards re-scrolls the window ([why](../wolf-core-decisions.md#a-saved-view-is-applied-twice)). So each restored edit pane holds its view as pending. Once no saved layout or drawer restore is pending, and every pane readiness counts is sized as it requested, the pane view sends each shown pane's view through `WolfCore::set_window_cursor`.

Each view applies once, and a later resize re-scrolls by Neovim's own rule. A pane a zoom hides keeps its view until it is shown and settled. Readiness waits until every shown pane's view is queued, so the view lands ahead of any typed key.

**A view still waiting to apply keeps its seed.** Until a restored preview's scroll is applied at its page's load the page reports the top, and until an edit pane's view is applied again Neovim shows a re-scrolled top line. A capture skips such a pane, so a save keeps the saved view rather than the passing one.
