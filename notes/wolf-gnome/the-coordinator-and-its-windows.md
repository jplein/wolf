# The coordinator and its windows

Last verified at commit `6298d01fb761da1bfdd64a0a25671a40b9d4e525`.

A process-scoped coordinator sits above the window-scoped shell, and exactly six things live up there, each because a second copy of it would be wrong rather than merely wasteful: the one `StateStore`, the one config read, the one `CellMetrics` measurement, the `app.`-namespace actions, the session's launch plans, and the test-only [readiness probe](../wolf-gnome-decisions.md#the-readiness-file), whose condition spans every window a launch restores. Everything else is per-window and stays there.

The config is read once so two windows cannot disagree about the font mid-session; the metrics because they are deterministic given the config and already shared by `Rc` inside a window; the plans because a launch restores a session rather than a window. The store's reason is sharper.

**The store is process-scoped because two stores over one directory clobber each other silently.** Measured: both report a successful flush, the file holds whichever tree was written last, each store still claims its own tree in memory, and a fresh open reads back the loser's. No panic and no corruption — one window's entire session is simply gone, non-deterministically. `wolf-ffi` records the same rule for macOS.

Each window contributes a `WindowState` subtree and the coordinator assembles the tree and commits it. A tab whose snapshot gate is shut contributes the subtree it last cached rather than vanishing, and **an empty contribution set commits nothing at all** — never an empty tree, which would overwrite a whole saved session with a well-formed file holding no windows. That rule holds at two levels: a window with no contributing tab drops out of the set the same way.

Closing one window has three answers, and each guards a different way to lose a session. Closing the last window is quitting, so its subtree is kept and the session survives — the path `:qa` takes. Closing one of several drops that window, or one the user deliberately dismissed reappears at the next launch.

A close arriving during a quit commits nothing at all. The quit path has already committed the whole tree once, while every core and every pane was still alive, and then closes each window in turn; committing again per window would shrink that tree to nothing one window at a time.

**The `app.` rows are installed once for the process and every window's shell installs `win.` rows only.** `gio::ActionMap::add_action` replaces an existing entry, so installing `app.quit` per window would leave the last window's closure owning the whole application's Quit — and closing that window would silently kill Ctrl+Shift+Q for every window still open.

**Neither of the crate's two display-global CSS providers is installed from up here, and the two follow opposite policies below it.** [`tree_view.rs`](../../crates/wolf-gnome/src/README.md#tree_viewrs) adds one per window for its current-file bold, so every extra window adds another identical provider, inert since the rule resolves the same either way. [`overlay_style.rs`](../../crates/wolf-gnome/src/README.md#overlay_stylers) adds one per process behind a thread-local flag, reaching the effect a hoist would have without the coordinator needing to know that three overlays share a stylesheet.

What holds `tree_view`'s call where it is, is evidence: the class reaches no pixel, and [`tests/README.md`](../../crates/wolf-gnome/tests/README.md) records that for the bold, so moving it would be an unverifiable edit to a startup path. That argument bears on where a call lives, not on whether it is guarded — which is the distinction that lets `overlay_style` carry a flag while `tree_view` carries none.

New Window opens a fresh, empty window at `$HOME` with its own core, inheriting nothing from the window it was asked from: not the working directory, not the layout, not the size. That is what macOS's `makeNewWindow` does and parity is the row's whole subject; inheriting a project is a new tab's behaviour.

**Restore is positional, never grouped by working directory.** macOS groups restored windows by directory because several of its views share one project's Neovim; here one window owns one core, so there is nothing to group, and inventing a grouping would merge two windows the user opened deliberately. A saved tree holding two windows on one directory restores both, and `MAX_RESTORE_WINDOWS` caps a corrupt or hand-edited file — it is a restore policy against forking dozens of Neovim processes before the first frame, not a limit on the schema.

**Raising a window is best-effort on Wayland**, and that is the platform rather than this call: a client cannot raise itself, so `present()` is a request the compositor may decline. Measured under `cage`: two toplevels exist and the newest stays on top, and presenting the backgrounded one raises nothing. A session compositor normally carries it through xdg-activation. Nothing may assert on it here.

Two windows on one project collide on the durable swap directory, and that is left alone deliberately. The swap directory outlives the process on purpose — that is what makes swap recovery work — so scoping it per core would break a shipped feature to silence a warning, and Neovim's own `W325` toast is the honest report. Windows are not deduped by working directory for the same reason restore is not grouped.
