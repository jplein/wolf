# Crash and stall recovery

Last verified at commit `63cb77c2575e673a9cc14565b296da20f028f908`.

Core reports each tab's captive Neovim's health as `Alive`, `NotResponding`, or `Dead`, and this front-end reads it at the top of every one of that tab's wakes, before any painting. `Alive` paints and resets the tab's attempt budget. `Dead` skips the paint and schedules one automatic in-place restart of that tab, and `NotResponding` does nothing else; on the selected tab, they show the opaque crash overlay and the stall overlay.

**A wedged Neovim is never restarted automatically.** A restart force-kills the process, so everything not already in a swap file is lost, and that decision belongs to the user's button press alone.

The overlay is opaque because its job is to hide the frozen last frame, so a user can tell "Neovim died" from "Neovim is idle". Its restart button is present in both modes and drives the same function the automatic path does, on the selected tab.

**Liveness, the restart and its budget are per tab, while the overlay is per window.** The overlay shows the selected tab's reading, and a switch re-applies it for the tab coming on screen. So a background tab's crash restarts that tab with no overlay and no banner, and no other tab is touched: the banner flashes only when the restarted tab is the selected one. A budget per tab is what keeps one tab's exhausted attempts from refusing another tab's first automatic restart.

**The automatic restart is deferred by a short timer rather than run inline, and the mechanism was measured rather than reasoned about.** The restart call is synchronous, so running it inside the wake handler blocks the main loop for the whole respawn with the overlay shown but never presented. A GLib idle does not fix that under GDK and Wayland — the frame clock has no ready paint source at the instant the handler returns, so the idle wins — while a real timer does.

**The attempt cap exists to stop a retry loop, never to refuse a user.** A failing respawn re-marks the core dead and fires another wake, which without a cap re-enters indefinitely. An explicit button press resets the selected tab's counter, at the documented cost that a failed manual restart buys one extra automatic respawn.

Only the open-file set comes back, reopened with the active file last because reopening is a sequence of ordered `InputEvent::Open`s. The pane topology is not restored across a restart, and neither is cursor or scroll position; do not confuse that with a quit and relaunch, which does restore the topology from the saved window state. The live terminal sessions come back as well, into the restarted tab, the only tab they belong to: every one but the drawer's as a new vertical split, then the drawer if it was open ([when they run](../crates/wolf-gnome/src/pane_view.rs.md#terminal-restore)). That tab's pane view is reset first, which is load-bearing rather than cosmetic: a fresh Neovim re-attaches at the configured initial size, and only a widget allocation change tells core otherwise.

**Never issue a blocking RPC on the restart path.** Against a dead Neovim it freezes the main loop for the whole timeout budget on every crash. The open-file mirror is read out of the snapshot instead.

Core marks a stall from a synchronous call's timeout arm and from its input pump, when one of its calls outlasts the budget with no Neovim prompt showing. Nine synchronous calls are reachable from this crate, counting Back and Forward as the one navigation move they are. One is the layout rebuild, which a restored tab issues on its own first wake with a pane, against its own Neovim.

**A restore costs one Neovim process per tab**: each tab's `WolfCore::start`, a spawn plus its setup calls, runs on the main thread before its window is presented, and each tab carrying a layout then issues one rebuild. `MAX_RESTORE_NEOVIMS` caps the tabs, and so the processes, one launch restores across every window, so a corrupt saved window state cannot fork dozens of Neovims; a window that only partly fits is truncated to its first tabs. `MAX_RESTORE_TABS` caps one window's tabs, and so its launch cost. None of these calls is reachable from the restart path.

Starting a tab's core stays outside the count, at restore, on New Tab and on New Window alike: no liveness exists to gate it on until the start returns, and a failed start opens no tab.

Eight are reachable from a running window: the buffer picker's candidate snapshot on Ctrl+Shift+E; a per-pane navigation move on Alt+Left or Alt+Right; the Copy yank on Ctrl+Shift+C; opening a Markdown preview and returning from one to its source, both reached from the command palette or from `:WolfPreview` / `:WolfEdit`; repointing a bound preview at a file, from a link clicked in it or a search result activated over it; rendering that document for the loopback server behind Preview in Browser; and the preview render, the one of the nine a burst of typing reaches, and coalesced rather than issued per keystroke.

[The preview render's budget](../wolf-gnome-decisions.md#the-preview-renders-budget) describes that coalescing.

Paste adds none: `gdk::Clipboard::read_text_async` resolves on a later main-loop turn, and the insert is fire-and-forget on the ordered input channel. A terminal pane's session calls add none either, Copy's `terminal_selection_text` included, since none of them waits on Neovim. `WolfCore::open_terminal` does block, and stays outside the count because only the test-only `WOLF_SMOKE_TERMINAL` hook issues it, once per window at launch ([Terminal panes](terminal-panes.md)). The sidebar's re-root adds none either: `WolfCore::live_cwd` is a `parking_lot` read of a tracked cell, taken once per wake and shared with the title refresh.

The eight calls a running window issues are gated in the same shape, each on the liveness of the core it calls into. `buffer_picker::open_decision` refuses to open unless the selected tab's core reads `Alive`; `nav_ui::nav_press` refuses a move on the same `Alive`-only arm; `clipboard_ui::copy_decision` refuses the yank on it too; and `preview_ui`'s `preview_press`, `edit_press`, `may_render`, `preview_file_press` and `browser_press` refuse the five preview calls on it, with `search_panel::activation_route`'s `Alive` arm gating a search result's repoint. None can prevent a first stall, and none may be strengthened until it can — what they prevent is re-probing a Neovim already known to be wedged.

`may_render` alone among the eight is read twice per render, because the call it admits can be deferred past the reading that admitted it ([The preview render's budget](../wolf-gnome-decisions.md#the-preview-renders-budget)).

The palette's Open Recent rows add none: reading the candidates is a SQLite read plus a bounded `stat` walk over the store's capped contents, taken at open and never per keystroke, so the inventory is unchanged.

The search panel adds no call of its own. `start_search` resolves its root from the tracked-cwd cell and hands `rg` to a worker, `open_at` is fire-and-forget on the ordered input channel, `liveness` is an atomic read, and a result over a preview reaches the `preview_file` counted above. `WolfCore::ignorecase()` is a blocking call and is deliberately never issued: its entire product would be the seed for the case toggle, which starts off instead, and it would buy that with a further route into the state the paragraph below describes.

Save and Save As add none either, so the inventory above stands at nine. Both ride the ordered input channel and the native dialog resolves on a later main-loop turn, so neither can time out. They are gated on `Alive` all the same, for the paste path's reason rather than the blocking one, and Save As carries a second reason of its own: a native modal over the opaque overlay covers the Restart button, which the paragraph below records as the only route out of a stall.

**Nothing in this crate clears `NotResponding`.** Core's input pump clears a stall once Neovim answers a call or a probe it was waiting on, and the overlay never takes keyboard focus, so typing into a stalled window dismisses the stall once Neovim recovers, whichever call raised it. With no typing, a stall a blocking call raised stays: core's other clear rides the `Ok` arm of a later synchronous call, the timed-out one is detached, and all eight blocking calls a running window issues refuse to run in that state.

Against a Neovim that never recovers, the route back is the stall overlay's Restart button, which force-kills. The honest fix for the untyped case, if that ever becomes a user problem, is a non-blocking liveness probe in core; there is none.
