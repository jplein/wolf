# Crash and stall recovery

Last verified at commit `24af6404466d9cc099af52ccec8d7c89155bc6f5`.

Core reports the captive Neovim's health as `Alive`, `NotResponding`, or `Dead`, and this front-end reads it at the top of every wake, before any painting. `Alive` paints and resets the attempt budget. `Dead` shows an opaque crash overlay, skips the paint, and schedules one automatic in-place restart. `NotResponding` shows a stall overlay and does nothing else.

**A wedged Neovim is never restarted automatically.** A restart force-kills the process, so everything not already in a swap file is lost, and that decision belongs to the user's button press alone.

The overlay is opaque because its job is to hide the frozen last frame, so a user can tell "Neovim died" from "Neovim is idle". Its restart button is present in both modes and drives the same function the automatic path does.

**The automatic restart is deferred by a short timer rather than run inline, and the mechanism was measured rather than reasoned about.** The restart call is synchronous, so running it inside the wake handler blocks the main loop for the whole respawn with the overlay shown but never presented. A GLib idle does not fix that under GDK and Wayland — the frame clock has no ready paint source at the instant the handler returns, so the idle wins — while a real timer does.

**The attempt cap exists to stop a retry loop, never to refuse a user.** A failing respawn re-marks the core dead and fires another wake, which without a cap re-enters indefinitely. An explicit button press resets the counter, at the documented cost that a failed manual restart buys one extra automatic respawn.

Only the open-file set comes back, reopened with the active file last because reopening is a sequence of ordered `InputEvent::Open`s. The pane topology is not restored across a restart, and neither is cursor or scroll position; do not confuse that with a quit and relaunch, which does restore the topology from `state.json`. The live terminal sessions come back as well, into the selected tab: every one but the drawer's as a new vertical split, then the drawer if it was open ([when they run](../crates/wolf-gnome/src/pane_view.rs.md#terminal-restore)). The pane view is reset first, which is load-bearing rather than cosmetic: a fresh Neovim re-attaches at the configured initial size, and only a widget allocation change tells core otherwise.

**Never issue a blocking RPC on the restart path.** Against a dead Neovim it freezes the main loop for the whole timeout budget on every crash. The open-file mirror is read out of the snapshot instead.

Core marks a stall from a synchronous call's timeout arm and from its input pump, when one of its calls outlasts the budget with no Neovim prompt showing. Ten synchronous calls are reachable from this crate, counting Back and Forward as the one navigation move they are. One is the layout rebuild, which the launch path issues.

**A window restoring `T` tabs carries `2T - 1` of them before it is interactive**: one `create_tabpage` per tab past the first, and one rebuild per tab carrying a layout. The two restore caps bound that product, the calls are all on the launch path, and none is reachable from the restart path, which discards the queue rather than replaying it.

Nine are reachable from a running window: the buffer picker's candidate snapshot on Ctrl+Shift+E; a per-pane navigation move on Alt+Left or Alt+Right; the Copy yank on Ctrl+Shift+C; creating a tabpage, whose two triggers are New Tab on Ctrl+Shift+T and the restore materialization above; opening a Markdown preview and returning from one to its source, both reached from the command palette or from `:WolfPreview` / `:WolfEdit`; repointing a bound preview at a file, from a link clicked in it or a search result activated over it; rendering that document for the loopback server behind Preview in Browser; and the preview render, the one of the ten a burst of typing reaches, and coalesced rather than issued per keystroke.

[The preview render's budget](../wolf-gnome-decisions.md#the-preview-renders-budget) describes that coalescing.

Paste adds none: `gdk::Clipboard::read_text_async` resolves on a later main-loop turn, and the insert is fire-and-forget on the ordered input channel. A terminal pane's session calls add none either, Copy's `terminal_selection_text` included, since none of them waits on Neovim. `WolfCore::open_terminal` does block, and stays outside the count because only the test-only `WOLF_SMOKE_TERMINAL` hook issues it, once per window at launch ([Terminal panes](terminal-panes.md)). The sidebar's re-root adds none either: `WolfCore::live_cwd` is a `parking_lot` read of a tracked cell, taken once per wake and shared with the title refresh.

The nine calls a running window issues are gated in the same shape. `buffer_picker::open_decision` refuses to open unless the core reads `Alive`; `nav_ui::nav_press` refuses a move on the same `Alive`-only arm; `clipboard_ui::copy_decision` refuses the yank on it too; `tabs_ui::new_tab_press` and `tabs_ui::restore_press` refuse the tabpage on it, one per trigger; and `preview_ui`'s `preview_press`, `edit_press`, `may_render`, `preview_file_press` and `browser_press` refuse the five preview calls on it, with `search_panel::activation_route`'s `Alive` arm gating a search result's repoint. None can prevent a first stall, and none may be strengthened until it can — what they prevent is re-probing a Neovim already known to be wedged.

`may_render` alone among the nine is read twice per render, because the call it admits can be deferred past the reading that admitted it ([The preview render's budget](../wolf-gnome-decisions.md#the-preview-renders-budget)).

The palette's Open Recent rows add none: reading the candidates is a SQLite read plus a bounded `stat` walk over the store's capped contents, taken at open and never per keystroke, so the inventory is unchanged.

The search panel adds no call of its own. `start_search` resolves its root from the tracked-cwd cell and hands `rg` to a worker, `open_at` is fire-and-forget on the ordered input channel, `liveness` is an atomic read, and a result over a preview reaches the `preview_file` counted above. `WolfCore::ignorecase()` is a blocking call and is deliberately never issued: its entire product would be the seed for a case-sensitivity toggle this front-end does not ship, bought with a further route into the state the paragraph below describes.

Save and Save As add none either, so the inventory above stands at ten. Both ride the ordered input channel and the native dialog resolves on a later main-loop turn, so neither can time out. They are gated on `Alive` all the same, for the paste path's reason rather than the blocking one, and Save As carries a second reason of its own: a native modal over the opaque overlay covers the Restart button, which the paragraph below records as the only route out of a stall.

**Nothing in this crate clears `NotResponding`.** Core's input pump clears a stall once Neovim answers a call or a probe it was waiting on, and the overlay never takes keyboard focus, so typing into a stalled window dismisses the stall once Neovim recovers, whichever call raised it. With no typing, a stall a blocking call raised stays: core's other clear rides the `Ok` arm of a later synchronous call, the timed-out one is detached, and all nine blocking calls a running window issues refuse to run in that state.

Against a Neovim that never recovers, the route back is the stall overlay's Restart button, which force-kills. The honest fix for the untyped case, if that ever becomes a user problem, is a non-blocking liveness probe in core; there is none.
