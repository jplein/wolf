# The coordinator and its windows

Last verified at commit `745eed6c2c4aaf9461c0e622afe54d4bcd6f3885`.

A process-scoped coordinator sits above the window-scoped shell, and exactly eight things live up there, each because a second copy of it would be wrong rather than merely wasteful: the one `AppStateStore`, the window stacking order, the one config read, the one `CellMetrics` measurement, the `app.`-namespace actions, the session's launch plans, the test-only [readiness probe](../wolf-gnome-decisions.md#the-readiness-file), whose condition spans every window a launch restores, and the open channel's listener. Everything else is per-window and stays there.

The config is read once so two windows cannot disagree about the font mid-session; the metrics because they are deterministic given the config and already shared by `Rc` inside a window; the plans because a launch restores a session rather than a window; the stacking order because it ranks the windows against each other; the listener because a state directory has one socket, a second bind is refused as in use, and a request can name a tab of any window. The store's reason is sharper.

**The store is process-scoped because two stores over one `wolf.sqlite` lose a session silently.** A second store keeps its own unchanged-tree baseline, and each of its writes replaces the whole window state the other wrote. No panic and no corruption — one window's entire session is simply gone. `wolf-ffi` shares one store across macOS windows for the same reason.

Each window contributes a `Window` entry and its tab recency through `WindowHandle::cached_window`, and the coordinator orders the entries by the stacking order through `state_seam::window_state` and submits them. A tab whose snapshot gate is shut contributes what it last cached rather than vanishing, and **an empty contribution set commits nothing at all** — never an empty window state, which would overwrite a whole saved session with one holding no windows. That rule holds at two levels: a window with no contributing tab drops out of the set the same way.

Closing one window has three answers, and each guards a different way to lose a session. Closing the last window is quitting, so its entry is kept and the session survives — the path `:qa` takes. Closing one of several drops that window, or one the user deliberately dismissed reappears at the next launch.

A close arriving during a quit commits nothing at all. The quit path has already committed the whole window state once, while every core and every pane was still alive, and then closes each window in turn; committing again per window would shrink that state to nothing one window at a time.

**The `app.` rows are installed once for the process and every window's shell installs `win.` rows only.** `gio::ActionMap::add_action` replaces an existing entry, so installing `app.quit` per window would leave the last window's closure owning the whole application's Quit — and closing that window would silently kill Ctrl+Shift+Q for every window still open.

**None of the crate's three display-global CSS providers is installed from up here, and each follows its own policy below it.** [`tree_view.rs`](../../crates/wolf-gnome/src/README.md#tree_viewrs) adds one per window for its current-file bold, so every extra window adds another identical provider, inert since the rule resolves the same either way. [`overlay_style.rs`](../../crates/wolf-gnome/src/README.md#overlay_stylers) adds one per process behind a thread-local flag, reaching the effect a hoist would have without the coordinator needing to know that three overlays share a stylesheet.

[`titlebar_background.rs`](../../crates/wolf-gnome/src/README.md#titlebar_backgroundrs) adds one per window whose data is that window's own colour, replaced only when the colour changes and removed at close, so it is per window by necessity rather than by default.

What holds `tree_view`'s call where it is, is evidence: the class reaches no pixel, and [`tests/README.md`](../../crates/wolf-gnome/tests/README.md) records that for the bold, so moving it would be an unverifiable edit to a startup path. That argument bears on where a call lives, not on whether it is guarded — which is the distinction that lets `overlay_style` carry a flag while `tree_view` carries none.

New Window opens a fresh, empty window at `$HOME` with one tab and that tab's own Neovim. It takes the active window's normal size ([§7](../../specs/application-state.md#7-new-windows)) and nothing else of a window: not the working directory, not the layout. That is what macOS's `makeNewWindow` does and parity is the row's whole subject; inheriting a project is a new tab's behaviour.

**The active window is the stacking order's first, never the window New Window was asked from**, as §2 defines it. Its normal size is `WindowHandle::normal_size`, GTK's default size, which a maximize or a fullscreen leaves at the unmaximized size; the allocation would lend a maximized window's size. With no window to take a size from, the default size stands. The new window opens unmaximized where the compositor puts it, since a plan carries no maximized state.

**Restore is positional, never grouped by working directory.** Every tab runs its own Neovim at its own saved directory, so no two windows or tabs share anything a grouping could save, and inventing one would merge two windows the user opened deliberately. A saved state holding two windows on one directory restores both, and `MAX_RESTORE_NEOVIMS` caps the tabs a corrupt or hand-edited saved state restores across every window, frontmost first: it is a restore policy against forking dozens of Neovim processes before the first frame, not a limit on the schema.

**Raising a window is best-effort on Wayland**, and that is the platform rather than this call: a client cannot raise itself, so `present()` is a request the compositor may decline. Measured under `cage`: two toplevels exist and the newest stays on top, and presenting the backgrounded one raises nothing. A session compositor normally carries it through xdg-activation. Nothing may assert on it here.

**Restore therefore maps the windows backmost first, each after the one behind it.** `restore_session` builds and registers every window unpresented, seeds the stacking order from the plans, and presents them by `state_seam::present_after`: a window that is not minimized waits until the one built before it has painted or been unrealized (`WindowHandle::show_after`).

Windows presented in one burst map in no order GTK promises, and a compositor that focuses each toplevel as it maps, as `cage` does in `view_map`, turns every map into a focus. So the map order is the order the next save writes. The seed ranks the windows until those focuses arrive, and ranks a window that never gains focus.

Minimized windows are presented at once, and nothing waits on one. The stacking order puts them behind the others whatever their map order, and whether a window minimized before it maps ever paints is unknown, so a link waiting on one could leave every window in front of it unmapped. A quit cancels every pending present (`WindowHandle::cancel_present`), since `close()` does nothing to an unrealized window, which would keep its Neovims and map later.

Under `cage` the last-mapped window is the one keys reach, and the readiness probe's input window. A window planned minimized gets `minimize()` before `present()`, which GTK 4 documents as minimizing it right after it maps; whether mutter activates such a window first, unmarking it, is unmeasured.

Two tabs on one file share the durable swap directory, and that is left alone deliberately. The swap directory outlives the process on purpose — that is what makes swap recovery work — so scoping it per core would break a shipped feature. `shortmess+=A` keeps a live owner's swap quiet instead ([One Neovim per tab](../../specs/application-state.md#8-one-neovim-per-tab)). Restore, New Window and New Tab never dedupe by working directory, for the same reason restore is not grouped. Open Folder alone focuses a tab already open on the folder rather than opening a second, as [§6](../../specs/application-state.md#6-opening-a-folder) asks.

## Opening a folder

Open Folder lives up here because it spans windows: a tab in any window can be the one it focuses. The dialog, the empty state's button through it, and an Open Recent palette row all reach `AppShell::open_folder`, which canonicalizes the path and hands it to `Coordinator::open_folder`. That gathers every window's live tabs and asks [`open_folder.rs`](../../crates/wolf-gnome/src/README.md#open_folderrs) for the step.

A focus selects the tab §4's rule picks in its `adw::TabView` and raises its window, best-effort as above, and reads no project state. A fill and a new window read the folder's project state, which marks it used; a new window opens through the same `open_window` a restored one does, at the project state's size, else the active window's normal size.

The tab a fill replaces is the current one Open Folder is handed: the menu's window and its selected tab, or a request's current tab, which can be a background tab. A fill raises its window too.

**A fill replaces the empty tab; it never chdirs its Neovim.** Core's tracked cwd moves only when Neovim reports `DirChanged`, and the drawer's shell and a terminal leaf whose saved directory is gone start in that cwd, so a rebuild racing a `:tcd` would start them in the old directory. A gather between the chdir and the rebuild would also write a one-pane tab over the folder's project state.

So `TabsHandle::replace` opens a tab planned from the folder, its Neovim starting there, at the empty tab's page position, and only then closes the empty one through the ordinary close path, since closing a window's last page closes the window. When the new tab's core fails to start, the empty tab stays and nothing else happens.

A fill gives the window the stored size only when the empty tab was its only tab. It also gives the search panel the new tab's recorded fields, as a launch does, or the next `record_selected_sidebar` would overwrite the restored query with the panel's.

Every Open Folder, a focus included, records the folder through `WolfCore::record_recent_dir` on the core of the tab that ends on it. That tab's Neovim starts in the folder, and core records no launch directory.

## Opening from outside Wolf

The `wolf` command's requests ([§2](../../specs/cli-and-file-open.md#2-the-channel)) come up here because a request spans windows: its `WOLF_TAB_ID` names a tab of any window, and a file goes to its project's tab wherever that is. The first `activate` binds the listener and a main-loop task drains it on each wake, handing each request to [`open_request.rs`](../../crates/wolf-gnome/src/README.md#open_requestrs) with every window's live tabs and tab IDs, then acting on the route. A directory, and a file outside any named tab, take the Open Folder path above, so §4's focus, fill and new window are one implementation.

**The listener binds before `restore_session`, and its requests still run after the restore.** The drain task runs only once `activate` returns, so a cold launch restores the session before it handles the request that started it. Binding first sends the launching `wolf` its `Taken` before any window maps; binding after the restore delays that past the maps. Under `cage`, the cold-launch smoke primes the keyboard once `wolf` returns, and a window that maps before any keyboard exists is never given focus there.

**`quit` drops the listener right after latching `shutting_down`, and the last window's close drops it too.** A `wolf` connecting to an app that is exiting would otherwise get `Taken` for a request nothing handles. Requests drained while shutting down are dropped.

**Every raise from a request or from Open Folder goes through `WindowHandle::raise`, which presents a window only once it has painted.** A request handled during the restore's present chain could otherwise map a window ahead of the ones behind it, out of the saved order.

A file is queued on its tab's pane view and placed by `wolf_core::choose_pane` on a later wake, once a pane is bound, every restore step of the tab has run and `open_request::may_place` admits its Neovim. A refused gather or a crash restart keeps it queued, so a file sent to a tab Open Folder has just opened waits for that tab's restore, and one sent to a dead Neovim lands after the restart.

The pane a file lands in is made active at once, not on the wake Neovim reports the move, or keys would still reach the pane that was active, a terminal's session among them. `pending_focus` keeps `update_active_pane` from reading Neovim's previous window back meanwhile. The whole-tab split binds the window `WolfCore::open_split_left` answers to a new pane from `PaneTree::split_root_left`, since no `Split` intent names it.
