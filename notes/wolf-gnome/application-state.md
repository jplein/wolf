# Application state

Last verified at commit `63cb77c2575e673a9cc14565b296da20f028f908`.

`state.json` is the shared `wolf_core::state` schema, unchanged. Its shape is common to this crate and the macOS front-end, so a Linux-only change to it is a macOS re-verification obligation. `wolf_core::state_dir()` is the single resolution point, and `WOLF_STATE_DIR` overrides it.

Each window re-gathers every one of its tabs on every render wake of any of them, with no timer and no debounce, and once more as the first thing its close handler does; the coordinator assembles the tree from every window and commits it. A wake refreshes only the waking window and reads the others out of their caches, so the cost does not grow with idle neighbours.

That is affordable because two coalescers already sit beneath it: the store is a no-op on an unchanged tree, and the writer thread coalesces before it writes. The crate's no-polling property survives, and a kill loses at most one coalesce window.

**Every gather is gated, and the gate prevents data loss rather than churn.** A subtree built while Neovim is dead, before the first pane binds a grid, or before Neovim has reported its per-window buffer map would persist an empty layout and overwrite the user's saved one with nothing. The gate is read once per tab, against that tab's own core, and it is asked about every tab.

**A window persists every tab's subtree, in tab order, and a background tab's is as trustworthy as the selected one's.** Each tab is gathered from its own Neovim's snapshot: each leaf's content is resolved from `Snapshot::previews` and `Snapshot::win_buffers` by the pane's own Neovim window handle, so the gather reads live values with no RPC and never calls `pane_status`. A leaf whose window is in neither map refuses that tab's whole gather, and the tab contributes its cache.

**Tab order is page order — the order the bar draws — and never the order the tabs were opened in.** `AdwTabBar` lets the user rearrange the bar by dragging a tab, with no property that turns that off, and the restore path replays a saved window's tabs positionally, so persisting the order pages were created in would discard that arrangement at the next launch and then write the discard back.

A restored tab with no bound pane has no cache to fall back on and contributes a seed derived from the layout it was restored with, until its own first gather succeeds; for a tab whose layout bound no pane, that is never. A tab whose panes did bind is gathered live from its own Neovim from the wake after its layout is realized, selected or not. A restored tab whose plan carried no layout takes no seed, which is what stops a launch whose project has gone committing an empty tab over the saved one.

**Each tab persists its own Neovim's working directory, while restore starts every tab of a window in one.** The plan reads the window's directory off its first saved tab, as the sidebar preference is read, and every restored tab's Neovim starts there; a tab that was moved with `:tcd` comes back at its window's directory.

**Suppression is by content, never by arity.** A tab's open buffer paths have no persistence channel of their own — they ride only inside the layout's leaves — so a save side that skipped a lone leaf would silently stop persisting the single most common shape there is: one window, one tab, one pane, one file. The save path suppresses only an unnamed, non-preview lone leaf, and the restore path rebuilds any saved layout with no arity guard at all. That is the shape of a bug already fixed on macOS; do not reintroduce it here.

**Window position is deliberately never persisted, permanently.** A Wayland client is given neither its own position nor screen geometry, so there is no analogue of the macOS check that a restored frame still intersects a screen — a restored coordinate could not be validated even in principle. Only the size is restored, and the position is written as an explicit zero rather than a plausible-looking value a later reader might trust.

**Nothing about the content-width toggle is persisted, deliberately and permanently.** `b:wolf_full_width` is a buffer variable Neovim owns and re-emits on every `BufEnter` and `WinEnter`, and `wolf_core::state` has no field for it — adding one would change a schema shared with macOS for a Linux-only feature. It would also key by pane path what Neovim keys by buffer, so one file open in two panes would get two persisted answers and one live one. The honest consequence is that the toggle does not survive a quit and relaunch, which is parity: macOS persists nothing either.

`sidebar_visible` carries the user's preference, and is restored at launch. It is per window, where the layouts are per tab, so it is written into every tab and read back off the first. What is written is that preference and never what is on screen: the empty state hides the sidebar without changing what the user asked for, so persisting the visible flag would make one project-less session hide the sidebar of every later one. `state_seam::sidebar_is_shown` combines the two at apply time, and a restored `false` is always reversible — from the header bar's toggle group or from `Ctrl+Shift+B`.
