# wolf-gnome/tests/smoke_headless

The `smoke_headless` test binary, with [`main.rs`](#mainrs) as its crate root. It holds the smokes that launch the built app under `cage`, and the compositor-free guards over them. The crate's other tests, and what is deliberately not smoked, are in [`tests/README.md`](../README.md); what no test proves is in [What is still unproven](../../../../notes/crates/wolf-gnome/tests/what-is-still-unproven.md); why the smokes run this way is in [The headless smoke test](../../../../notes/wolf-gnome-decisions.md#the-headless-smoke-test).

## Files

#### `main.rs`

**Covers**

- The binary's shared harness. Its smokes launch the built application under `cage` and assert on OCR, pixels, readbacks, files or the exit status. Its helpers, `RESIZE_OUTPUT_FRAGMENT` and the prelude's `quit_app` among them, carry the [Smoke conventions](#smoke-conventions).
- The harness guards: `cage` spawns only through `cage_command`, each cage is isolated from the host session, the slots bound concurrency, every launch waits for readiness, the appearance pin holds, and the column cap cannot bind at the OCR geometry.

**Decisions**

- `CAGE_SLOTS` is per process, so a cage smoke belongs in this binary, where the slot bound counts it.

**Gotchas**

- `--exact` takes `<module>::<name>`, as in `editor::window_renders_typed_text_under_cage`; the guards here keep bare names.
- Every `*_under_cage` test is a cage smoke and every other test a compositor-free guard; `passed` counts both.
- The source-scanning guards read the crate's test sources at run time: a `cage` spawned under `tests/` outside `cage_command` fails one, and a declared module left unread fails both.
- A failing smoke keeps its runtime directory, with its captures and `cage.log`, and prints its path and the log's tail.
- Spell checking is off in every smoke, so OCR reads glyphs not undercurls; see `seed_smoke_spell_override`.

#### `close_pane.rs`

The Close Pane smoke, one launch restoring a planted window of two tabs of one pane each, each on a file of the project, with the selected second tab then modified. Ctrl+Shift+W there must keep both Neovims and leave E37 in its `:messages`. After `:w` it must close the tab, leaving the first, and in the last tab end the app. Every observable is a Neovim count, a readback a Neovim writes about itself or the app's exit, never OCR.

It clicks nothing, because keys reach the selected tab after a restore, a premise exit 191 checks.

#### `close_tab.rs`

The Close Tab smokes, each one launch restoring a planted window whose tabs are on files of the project. Every observable is a Neovim count, a readback a Neovim writes about itself or the app's exit, never OCR.

`close_tab_asks_the_tabs_neovim_under_cage` restores two tabs, with the second modified. A click on the background second tab's close button must select it, keep both Neovims and leave E37 in its `:messages`, and Close Tab from the palette must refuse the same way. After `:w` it must close the tab, leaving the first, and on the last tab end the app.

`close_tab_with_no_answer_on_the_last_tab_closes_the_window_under_cage` restores one modified tab whose Neovim's `_G.wolf_close_tab` is made to answer no boolean, so the ask gets `NoAnswer`. Close Tab from the palette must then end the app, because the last page's close confirms its tab and the window's check does not ask it again.

The close button's x is `CT_CLOSE_BUTTON_INSET_PX` left of the output's right edge, and a background tab shows its button only while hovered. Exit 184 or 185 can mean the click missed the close button, and then the inset is what to fix, not the code.

#### `close_window.rs`

The Close Window and Quit smokes, each one launch restoring a planted window of two tabs on files of the project, with the background second tab modified. Every observable is a Neovim count, a readback a Neovim writes about itself or the app's exit, never OCR.

`close_window_asks_every_tab_under_cage`: Close Window from the palette in the clean tab must keep both Neovims, select the modified tab and leave E37 in its `:messages`. After `:w`, `:qa` there must close only its own tab. Close Window on two clean tabs, the one left and a new one, must end the app with neither Neovim left.

`quit_asks_every_window_under_cage` opens window 2 from the palette, and a readback of its buffer's name proves the keys reach it. Ctrl+Shift+Q there must quit nothing. `:WolfCloseWindow` in clean window 2 must close it, which shows that Close Window asks only its own tabs. The keys then pass to window 1, whose readback and `:messages` show that the refused quit selected the modified tab and echoed E37, and after `:w` Ctrl+Shift+Q ends the app.

#### `compose.rs`

Input-method smokes: composed text reaching Neovim, and a control chord typed while a compose sequence is pending.

#### `config.rs`

Configuration smokes: `wolf-config.yaml`'s font size and appearance reaching the grid and Neovim, and the column cap with the full-width toggle.

#### `drawer_divider.rs`

The drawer divider's smokes. One launch drags the handle between the editor tree and the drawer through [Pointer injection](#pointer-injection), reading back Neovim's window heights and the drawer shell's `stty size`. The drag must grow the drawer's window and its session, survive the drawer closing and reopening, give the editor back its launch height while the drawer is closed, stop at the editor's floor when released above the pane area, and stay out of a second tab New Tab opens, whose drawer opens at a new share.

A second test, `a_dragged_share_survives_a_window_shrink_and_grow_under_cage`, drags the share to just above the editor's floor, shrinks the window until that share's position clamps to the floor, and grows it back. The editor's and drawer's heights must return to the drag's, so the shrink's clamp was not taken for a drag.

A third, `each_tab_keeps_its_drawer_share_across_a_relaunch_under_cage`, drags the drawer in one of two tabs. The other tab's drawer must open at a third, each tab must show its own share on a switch, both must be saved open with their own shares, and a relaunch must restore each.

No assertion reads a pixel.

#### `editor.rs`

Editor smokes: typed text, the file and buffer pickers, the empty state, a new untitled buffer, the command palette, navigation history, completion, messages and the confirm prompt, the clipboard, and saving. The multi-line message smoke also reads the toast's box from the capture's pixels: as wide as its widest line's cells, a wide character paying two, plus the padding on both sides, and no wider.

#### `extras.rs`

The per-tab and per-pane extras' smokes, each over the saved state frozen at its capture moments and none reading OCR:

- `pane_cursors_are_captured_and_restored_under_cage`: edit cursors and top lines captured on a focus change, a tab switch and quit, and not between them, then restored with each tab's focused pane.
- `preview_scroll_survives_a_close_and_relaunch_under_cage`: a wheeled preview's scroll captured as its window closes and applied after the relaunch's load. The relaunch wheels the restored page further, so its quit must save the restored scroll plus that wheel, and a restored scroll that is never applied does not pass.
- `each_tab_keeps_its_sidebar_page_under_cage`: each tab of one window saving its own sidebar page.
- `a_zoomed_focused_drawer_restores_under_cage`: a drawer saved open, zoomed and focused reopening that way with a shell in the project, at its saved share once unzoomed.
- `a_restored_top_line_survives_its_grid_resize_under_cage`: a top line restored with the cursor below the top row surviving the grid resize that lays out its pane.

#### `file_open.rs`

The `wolf` command's smokes, each over a planted state. They run the `wolf` beside the app binary, and fail when it is absent ([Building and testing](../../../../build.md#building-and-testing)). `wolf D` from the empty state fills the empty current tab with `D`, adding no Neovim. `wolf FILE` lands by each [§5.2](../../../../specs/cli-and-file-open.md#52-choosing-the-pane) rule, one tab per rule: the active editor pane, on a missing file left off disk; the larger leftmost editor pane; a preview made an editor; the pane already on the file focused; and a zoomed tab unzoomed first.

A terminal pane's shell sees its tab's `WOLF_TAB_ID`, distinct per tab, and its `wolf` lands in that tab through the whole-tab split; a stale ID goes to the file's project. With no app running, `wolf FILE` launches the app beside it, which restores both planted windows, adds none for the command's directory, and places the file after the restore.

`wolf --wait` serves `git commit` typed into a tab's terminal, committing the typed message on `:wq`, and one from outside Wolf, which `:cq` aborts with no commit. `:wq` on a tab's last pane, which closes that tab alone, still exits the waiting `wolf` `0`.

#### `fixture_guards.rs`

The guards behind the fixture-naming rule in [Smoke conventions](#smoke-conventions). A smoke that OCRs a capture registers each term it asserts on in `ASSERTED_OCR_TERMS`, which the guards check every fixture against; a smoke that OCRs nothing registers none.

#### `frame_stats.rs`

The frame-time instrument's smoke: typing bursts with `WOLF_FRAME_STATS` armed, read from the report file the application appends to.

#### `header_bar.rs`

The header bar's contents smoke: one keyboard-only launch, so no pointer hover reaches the capture, and one capture before any key. The header band's columns are read as ink against the bar's own background, the dominant colour of the span the titlebar smokes read as one flat fill. The run of ink holding the Search toggle's centre must start where the group follows Back/Forward, within half an image button, and nothing may lie right of it, so the start holds Back/Forward and the group alone, as the macOS toolbar does. Then `Ctrl+Shift+O`, a query and Enter must open the fixture, read back as Neovim's buffer name.

#### `lighthouse.rs`

Not a smoke: the oracle that reads expected colours from the pinned `colors/lighthouse.lua` in the tree `WOLF_TEST_LIGHTHOUSE_DIR` names, a copy of wolf-ffi's `common::Lighthouse` that parses the same shapes ([`//bazel/lighthouse`](../../../../bazel/lighthouse/README.md#the-pin)). It is read in the test process only: the packaged smokes scrub the variable from the cage. Its `stage_init_lua` stages the checkout's `init.lua` beside a copy of the pinned colorscheme, for a smoke that runs the real Lighthouse.

#### `new_tab.rs`

The New Tab smokes, each launching on a folder tab and opening a folder `D` through its Open Recent row, then reading the save the quit leaves. New Tab must open an empty tab beside it: a live Neovim at `HOME`, the empty-state page, no sidebar, and `wolf` in its bar entry and in the header once it is the window's only tab. A `D` with no project state opened into that tab must show the sidebar exactly when the origin showed it at New Tab, after the origin has closed too. A `D` whose project state hides the sidebar must hide it over a shown origin, and New Window's empty tab must show the sidebar whatever its origin showed.

No Ex command is typed before a capture in the tab it shows, since the cmdline strip would draw the runtime dir's path.

#### `open_folder.rs`

Open Folder and New Window smokes over a state each plants in its own store, by submitting a tree holding `D` and then one without it: an Open Recent row for `D` open in a background tab of the back window selects that tab and opens nothing; from an empty tab, fills it with `D`'s two-pane split; from a folder tab, opens a new window restored to that split and its stored size, leaving the launch tab alone.

The [§7](../../../../specs/application-state.md#7-new-windows-and-tabs) smokes plant a launch window at a size neither the default nor the output's: New Window, and an Open Recent row for a folder with no project state, each open a window at that size. Under `cage` the launch window is maximized, so New Window also proves a maximized window lends its normal size. They read the save the quit leaves and a live `getcwd()` / `winnr('$')` readback, never pixels: a tab contributes its seed until its first gather succeeds, so a save can echo a restore that never applied.

#### `packaged.rs`

The `#[ignore]`d packaged smokes, which drive the store binary `nix build .#wolf-gnome` installs, scrubbing every devShell export the artifact supplies itself: a clean OCR launch, an A/B against the checkout binary over the two vendored Lua trees, a preview pane, and a terminal pane whose shell gets `TERM=xterm-ghostty` and the package's `TERMINFO`, with `infocmp -x xterm-ghostty` succeeding there. The A/B reads Lighthouse's colours through [`lighthouse.rs`](#lighthousers). They run only through `./tools/wolf-package-smoke.sh`, which selects them with the filter `packaged::`.

#### `panes.rs`

Pane and layout smokes: splits, focus moves, closing panes, `:q` and `:qa`, the per-pane status bar, and native tabs, where each tab must run a Neovim process of its own, New Tab's must not start in the selected tab's directory, and closing one must shut that Neovim down. A file saved in one tab must reload in another when it is selected, through the focus trigger's `:checktime` alone.

Of two tabs, `:q` on one's last pane and `:qa` in a split one must close that tab alone, leaving the window and the other tab's Neovim. In a one-tab window `:qa` closes the window, through `tabs_ui::close_outcome`'s last-tab arm.

#### `pointer.rs`

Pointer smokes, driven through [Pointer injection](#pointer-injection) and read back from Neovim: click, drag, wheel scroll, click-then-close keyboard focus, and a divider drag across a relayout.

#### `preview.rs`

Markdown preview smokes: opening a preview and returning to the source, a link click that also makes the preview it repoints active, a click in a preview's page, or in its top padding gutter, making it active, read from Neovim's current window and from the strips' pixels, an unsaved buffer, the configured appearance, a view created after launch, Ctrl+Shift+P and Ctrl+Shift+W still firing after a click in the preview, and Preview and Edit in Another Pane reusing the other pane, where a press from a lone pane splits. Its guard keeps the smokes' burst delay below the coalescer's trailing delay.

#### `preview_margins.rs`

The preview's content-width smoke, read from raw pixels and the frozen window state, with no OCR. One launch at `FULLWIDTH_SMOKE_FONT_PT`, where the cap binds, previews a code block above several viewports of one-line paragraphs in a lone pane. The page's background must span the pane below the strip with the code block on the capped grid rect, `:WolfFullWidth` must widen the block to the pane less its padding, and a second toggle must bring it back. A wheel over the left side margin must then scroll the page, read from the saved preview scroll; every failed assertion is reported by its ID.

#### `preview_render.rs`

Preview rendering smokes read from raw pixels: fenced-code highlighting, the code block's background in each configured appearance, the baseline grid under inline code and the baseline below h3, h4 and h6 headings, and the file picker drawn over a live preview, with guards on the thresholds those pixel readings use.

#### `recovery.rs`

Recovery smokes: a crashed Neovim restarted with its file, a stalled one raising the stall overlay, the crash overlay's Restart button, and a background tab's Neovim killed while another tab is selected, which must restart that tab alone with its file while the other tab's Neovim lives on and the window keeps both tabs.

#### `restore.rs`

Session-restore smokes, each two launches in one cage, the second reading what the first saved: the pane layout, a lone file, tabs, each restored with a Neovim of its own that holds its own file, dragged divider weights, and two windows, saved in stacking order with their tabs, active tabs and a zoom, and restored with the frontmost in front on its saved active tab.

Two more launch once: over a stale `state.json` and `state.json.tmp`, which must be gone once the app is ready, with nothing of them restored; and over a planted state of three windows, which the save at the quit must list in the planted order.

#### `scrub_lists.rs`

The packaged smokes' scrub lists, the devShell exports removed from a packaged launch. [`flake_exports.rs`](../README.md#flake_exportsrs) compiles this file through `#[path]`, so its guard reads the lists the packaged smokes use; which list takes which export is in the [`flake.nix` notes](../../../../notes/flake.nix.md#shellhook-exports).

#### `search_toggles.rs`

The search panel's toggle smokes, one each for case, regex, an invalid regex such as `foo(bar`, which must show rg's error in the status line and then be found literally with regex off, and all files, where the panel opens Markdown-only and the Enter after `All` goes on finds the `.txt` too. What a search found and what the status line says are OCR of the sidebar band. Where the toggles sit, left-aligned below the field and above the results, is the changed rows and columns between captures of one run, read from the band's raw pixels.

The click targets are estimated from libadwaita 1.9.3's stylesheet and a measured label width, not from a probe, which is safe only because a click that misses a toggle changes no option and fails.

`a_restored_search_sidebar_runs_its_query_under_cage` launches over a tab saved on its Search page: the band must show what its query and toggles find, the save must hold them, and a later edit of the field must reach the save at quit.

#### `sidebar.rs`

Sidebar and search smokes: the file tree, the project-search panel and its chord, the sidebar toggle, a typed `:cd` re-rooting the sidebar, an Open Recent row from a folder tab opening a fresh window on a folder with no project state, and the window title.

#### `sidebar_toggles.rs`

The header bar's toggle-group smokes. One clicks the toggles through [Pointer injection](#pointer-injection) to switch, hide and show the sidebar, a Search click putting typed keys in the field; the other moves the group with Ctrl+Shift+B, Ctrl+Shift+F and `:WolfSidebarShowProject`. The sidebar's state is OCR of the sidebar band, and the group's is the raw pixels of the header bar's start, compared between captures of one run, so neither smoke navigates after its first capture.

The click targets are derived from libadwaita 1.9.3's header metrics, not from a probe, which is safe only because a click that misses the group moves no sidebar and fails.

#### `split_commands.rs`

The split-command smokes: from an editor, a terminal and a preview pane, the palette's Split Vertical and then, in the new pane, Split Horizontal must save a root of `vertical[pane, horizontal[pane, pane]]` whose every pane is of the first pane's kind. A saved pane carries no session id, so a new session is wolf-core's `split_commands.rs` question, and so is the drawer, whose split the front-end drops.

#### `symbols.rs`

The symbols-face smoke: one launch at the smoke config shows `M` and the icons U+F07B, U+E0A0 and U+F401 in an editor buffer, set through `nr2char`, and in a `:terminal` whose shell prints them. In each `grim` capture, `M` must match DejaVu Sans Mono alone, every icon the symbols face alone and not the hex box, and U+F401 must differ from DejaVu Sans Mono alone. The references are painted in the test process through `wolf_gnome::paint`, on the surface kind each pane paints on, and the preconditions hold the grid origin and the cell to the values the pointer smokes use.

#### `tab_bar.rs`

The tab bar smoke: one launch restores a planted window of two tabs on sibling directories under the smoke's `HOME`, so neither is the other's prefix. Both tabs hide the sidebar, whose root row would also draw the selected tab's directory, so only the bar and the header draw one. The header draws the selected tab's, so each OCR read is of a background tab's bar entry: tab 0's after its own `:cd`, and tab 1's after a `:cd` it runs in the background, which must replace its old title.

#### `terminal.rs`

Terminal smokes: the terminal drawer, terminals across a Neovim restart, terminal panes beside Neovim panes, typed keys reaching a pane's shell, Ctrl+`` ` `` from an editor focusing the open drawer, which a second Ctrl+`` ` `` then hides, Ctrl+`` ` `` hiding a showing message toast as it opens the drawer, and Ctrl+Shift+W closing the drawer with its session kept and a terminal split with its session hung up. Two more cover the tab's last pane beside the drawer: `:bd` in the only editor pane keeping it, current, on the earlier file, and able to take focus again after the drawer has had it; and a file the picker or a search result opens while the drawer is focused landing in an editor pane, the search result at its line, with the drawer kept.

#### `terminal_colours.rs`

The terminal-colours smoke: two launches of the checkout binary in one cage, over a staged `init.lua` beside a copy of the vendored Lighthouse, with `appearance` set to `dark` then `light`. In each, a pane's 16 ANSI background cells must paint Lighthouse's colours for that background, read through [`lighthouse.rs`](#lighthousers), and every run prints a table of expected and measured colours. The cells' pixel rects are derived from the pane geometry, with no pixel literal.

#### `terminal_terminfo.rs`

The terminfo smoke: the checkout binary, given `WOLF_TERMINFO_DIR` from the devShell's `WOLF_TEST_TERMINFO_DIR` and decoy `TERM` and `TERMINFO`, opens a pane by typed `:terminal` whose shell reports `TERM=xterm-ghostty` and that `TERMINFO`. It proves that `window.rs` hands core the directory; core's choice is wolf-core's `terminal_terminfo.rs`. Its shell script, scenario and readback are shared with the packaged terminfo smoke.

#### `titlebar.rs`

The titlebar smokes, one per Lighthouse background: the checkout binary over a staged Lighthouse, with `appearance` forced to that background while `ADW_DEBUG_COLOR_SCHEME` makes the desktop prefer the other. One launch opens a second tab and takes four captures: focused, after `:set background` to the other, after a runtime `WolfTitlebar`, and in backdrop with GTK's in-process save dialog up, so the smoke exports `GDK_DEBUG=no-portals`. The header bar and the tab bar must match the grid's `Normal`, then `WolfTitlebar`, and the title and the tabs' text must be drawn on the forced scheme's side.

The dialog is the route to backdrop, since no launch starts inactive under `cage`. A precondition proves the dimming: the title's contrast over `WolfTitlebar` must reach a floor while focused, and be at most a share of it in backdrop.

The empty-view smoke opens an empty tab over Lighthouse under `appearance: dark` and samples the view's two side bands. The view must match the grid's `Normal`, then the other background after a typed `:set background`, and keep it after a runtime `WolfTitlebar` the header bar takes. An OCR precondition proves the view is up.

#### `titlebar_tabs.rs`

The `gtk-titlebar-style` smokes. The first launches on the shared seeded config, so it runs the default `tabs`. With one tab, the header band must read the window title and the probe click the one-tab pane origin. A second tab, `:cd`ed into a sibling directory, must put both tabs' titles in the header band, take no editor row, leave the header's shade row where it was by `TABS_HEADER_GROWTH_PX`, and paint the tabs in the grid's background; a probe click and a delta click in it must land.

A middle-click on the second tab's header entry closes it, and the title, the header's height and typed input must all return. The second smoke pins `native`: with two tabs, the header must read the window title alone, the row below it both tabs' titles, and the editor lose rows. `titlebar_pins_parse_to_their_styles` is a compositor-free guard that the shared config and each pinned one parse to their style with the smoke font kept.

The header band is cropped down to each capture's own shade row, measured rather than assumed, so a header that grew moves it.

#### `tree_reveal.rs`

The sidebar launch-reveal smoke. One launch restores a tab with the tree sidebar beside an editor pane, focused, and a preview pane, both on a file below the fold, over a generated folder of about three hundred mixed-case root files, four subdirectories and the usual dot entries. Capture A must read the six rows directly above the current file and its own row; the scenario then creates a file, so the watcher's rescan rebuilds the tree, and capture B must read them again and the new row. The scenario sends no key and no pointer event, so nothing but the reveal scrolls the list.

The asserted rows are the subject, so their generated names carry their terms. `reveal_fixture_puts_its_rows_directly_above_the_current_file` guards that premise without a compositor: the rows' order under `wolf_core::tree::children_of`, the row counts above and below, and each term naming one row.

#### `zoom_pane.rs`

The Zoom Pane smokes. One launch builds `vertical[A, horizontal[B, C]]` over the drawer, drags the tree divider and the drawer handle, then zooms B from the palette. B must fill the pane area while the others keep their size, and each unzoom must restore every pane, both drags included. Zoom Pane again, `<C-w>h`, Neovim's own `<C-w>w`, Toggle Terminal focusing the open, hidden drawer or opening a closed one, a split and Preview in Another Pane must each unzoom, and a focus move must land on the pane it names in the unzoomed layout.

A second test, `a_zoomed_pane_survives_relaunch_under_cage`, quits with a pane zoomed and relaunches. Run 1's save must zoom that pane, asserted before run 2 so a lost save is told apart from an ignored restore, and run 2 must start zoomed on it.

A third, `an_unzoom_after_a_window_resize_fits_the_new_size_under_cage`, zooms B over the same dragged layout and resizes the window. B must fit the new size while the hidden panes keep theirs, the unzoom must match a plain resize to that size, and both drags must survive the return to the launch size.

Every observable is a Neovim `winwidth` or `winheight` readback, the grid size or the save, never a pixel.

## Smoke conventions

The cage slots and private runtime directories, the per-cage session bus that activates nothing, the pinned GTK settings, the single-shell-command shape, and the single environment helper are in [wolf-gnome-decisions.md](../../../../notes/wolf-gnome-decisions.md).

**A fixture must not be named after the term its smoke asserts on.** The sidebar renders every file in the working directory by name and nvim's `:e` echoes the filename, so a fixture carrying the asserted word satisfies the assertion whether or not the feature did anything — a permanent silent pass, not a flake. Fixture names and asserted terms are therefore deliberately unlike: a heading lives only in a file's content. The exceptions are the smokes whose subject is a tree row, such as the sidebar-population and launch-reveal smokes, where the rendered row is the feature under test; [`fixture_guards.rs`](#fixture_guardsrs) lists each such term.

That rule has three mechanical guards, because the fixture and the assertion it can disarm sit in different files: one over the sidebar filler names, one over the temp working directory's fixed path components, which the header bar renders as page text, and one over the smoke-owned relaunch fixtures.

The third reaches past names to the fixture bodies an assertion's line, column or rendered text is derived from, and to the crossed pairs behind every exemption, in which neither member may carry the other's term. Each guard's tables are built from the assertion constants rather than retyped, so they cannot drift, and every term is checked lowercase because the OCR output is lowercased before comparison.

**A fixture must not sit where something other than the feature would answer for it.** The preview-link smoke's source and target live in a subdirectory, not at the project root, because the captive Neovim resolves a relative path against its working directory: with both at the root, a missing base URI resolves the raw href to the same file the correct base does, and the smoke stays green with the feature reverted. Moving either fixture back to the root silently disarms it, which is why a guard asserts the subdirectory.

**A red-run demonstration is a deliverable, not a formality.** A check can be aimed at the right property and still look somewhere the defect cannot appear: a press-and-release at two different cells enters visual mode by itself, so a drag assertion needs a round trip whose press and release are at the same cell to discriminate drag events at all; a wrapper that refuses to exec is never named `nvim`, so a child-name scan cannot see an uncapped retry loop; and a centered button spans the same midpoint whatever width it requests, so a center-derived click point is blind to a width regression. Only running the revert exposes any of them.

**A scenario ends at its first failed readback, so a falsifier's red leg is measured, not predicted.** A scenario exits as soon as a readback it waits for fails to appear, and the Rust side stops at its first failing assertion, so later legs are never evaluated and an edit aimed at one leg can go red at an earlier one. A falsifier's leg table therefore names the disabling edit, and the coder records which leg went red after running it.

**Exit codes carry meaning, and folding two causes into one code destroys evidence.** A process that vanishes because the main loop panicked looks exactly like one that shut down cleanly, so "exited non-zero" gets its own code rather than joining "still alive"; likewise "the app died with its Neovim", "not exactly one live child", and "the killed child never left the process table" are each distinct.

**A scenario whose quit only ends its run calls `quit_app "<label>"`.** Quit refuses while a tab holds a modified buffer ([§9](../../../../specs/application-state.md#9-closing-a-window-and-quitting)) and `:qa` closes one tab, so the helper clears `modified` on every buffer of the Neovim the keys reach and types `:WolfQuitApplication`; another tab's modified buffer still refuses it. It exits 241 when the app outlives its wait and 242 on a non-zero exit, and its stderr label names the launch. A one-window app's Quit commits the tree its last window's close would, so a relaunch smoke may use it.

**A smoke closes a tab by its close button or a middle-click only once the tab is unmodified.** The close asks the tab's Neovim, which refuses while any of its buffers is modified, so `panes.rs`'s native-tabs smoke and `titlebar_tabs.rs`'s first smoke type `:set nomodified` in the closing tab first.

**A message smoke may press no key after the one that produced the message.** Any `Key`, `Mouse` or `Scroll` is dismissing input for a non-prompt toast in `wolf_core`, and so are the drawer toggle, a focus move into a terminal pane and input to one, so a stray keystroke or chord between the command and the capture clears the surface the smoke exists to read. The completion popup has the same shape for a different reason: the next key closes it.

**No scenario sends input before `wait_ready`.** The environment helper prepends one shell prelude to every scenario and sets `WOLF_READY_FILE`; after each launch the scenario calls `wait_ready "$app_pid" <died-code> "<label>"`, which primes the seat's keyboard with a key-free `wtype ""` and returns once the file holds that pid — the app writes it when its windows accept keystrokes ([wolf-gnome-decisions.md](../../../../notes/wolf-gnome-decisions.md#the-readiness-file)). A dead app exits with the scenario's own died-early code, and a launch that never gets ready exits 40 after 30 s; the app's `wolf-ready:` stderr lines then name the condition it was stuck on.

There is no fixed launch budget before `wait_ready` and no delivery margin after it: a key lost past readiness is a missing condition that a margin would hide. A sleep that waits for something else may still follow it, as `TREE_SETTLE` does for the sidebar's file search, and `SETTLE` is the settle between steps. `every_launch_waits_for_readiness_before_its_first_input` checks the launch side only, and nothing mechanical keeps a margin out after the call.

**Every smoke launches in a fresh temp working directory, and that is mandatory.** The captive Neovim keeps a durable swap directory keyed on the cwd, which deliberately outlives the process so a fresh nvim can recover a crashed one's unsaved edits. At a fixed cwd, one run's leftover swap file makes the next run's startup buffer raise a blocking prompt that eats the first keystroke, and the smoke goes red for whatever change happens to be in flight.

**A hung smoke does not fail, it hangs.** A scenario's trailing `wait "$app_pid"` carries no timeout, so a wedged run holds its cage slot indefinitely and the binary never finishes. The other smokes keep running on the remaining slots, though at `WOLF_SMOKE_JOBS=1` everything waits behind the hang. Libtest's `has been running for over 60 seconds` line is a hint rather than proof: it also names a smoke that is only waiting for a slot, or one that is slow under load.

**A failure that appears only when the cages run together is most likely load, not isolation.** Each cage has its own runtime directory, Wayland socket and session bus; what the cages share on purpose is the shader cache in `XDG_CACHE_HOME`, the fontconfig cache and the X11 display numbers. Reproduce it with [`WOLF_SMOKE_JOBS=1`](../../../../build.md#building-and-testing) first, then under load, and read the kept runtime directory before touching a settle.

**A smoke that can reach a native `gtk::FileDialog` must export `GDK_DEBUG=no-portals`.** Whenever the app reaches a session bus GTK takes the desktop-portal route, and `cage` implements no `zxdg_exporter_v2` to export a window handle through, so the request goes to whichever portal that bus offers. Each cage's bus is private and activates no portal, and the export keeps GTK's in-process chooser, the one the smokes were measured against, without resting on that; if the isolation ever regresses, it is what stops a modal file dialog opening on the developer's own desktop and hanging the smoke.

`no-portals` forces GTK's in-process fallback, which was measured to present under `cage` and to answer its callback. It is per-smoke scenario environment and never production behaviour, and it belongs on every smoke whose red path could open a dialog, as well as on every smoke whose green path does.

**A palette-driven smoke depends on a ranking it cannot see, and one such dependency is live.** No query separates `Preview` from `Preview in Browser` and `Preview in Another Pane`, and a command MRU carrying `preview_in_browser` would put that row first, so Enter would hand the document to a browser. The preview smokes are safe only because each run gets a fresh `WOLF_STATE_DIR` and none of them ever activates that command; `palette::tests::palette_rows_for_the_preview_smoke_rank_the_named_command_first` says why and asserts the inversion.

The other two preview queries carry no such dependency: `prvan` and `ednap` each match exactly one title in the whole registry, so they rest on uniqueness rather than on a ranking. Every smoke needing the preview beside its source types `prvan`, because plain Preview binds into the active pane and from a one-pane window would leave no editor half for a control band to sample.

**The rendered page must be identical on every run, not merely dense.** The temp cwd is a unique parent holding a fixed-name child, and the helper hands the parent to the app as its home directory, so the title seam abbreviates the varying part away and every header bar renders the same string. A run-varying title makes an unrelated, correct smoke stochastic. Sparse pages have the opposite failure: an isolated leading glyph is misread as a ligature, which is why several smokes drop filler files purely for density. If that regresses, add filler files rather than weakening an assertion.

**The block cursor corrupts the cell it covers, and the default page-segmentation mode reads nothing here.** A new scenario parks the cursor off the asserted row with a normal-mode `o` then Escape, because the inverted block turns the glyph beneath it into noise, and keeps the sparse-text segmentation the helpers pass, because the default mode reads nothing at all on the light-on-dark grid.

**A terminal marker must not share a row with the shell prompt.** Resizing a pane, whether by a drawer opening below it or a split halving it, makes readline redraw its prompt row from column 0 and clear it, erasing a marker printed there. End the marker's line, as `; echo` does, so it sits on a row of its own.

**A colour claim needs pixels, not OCR.** `tesseract` cannot tell light-on-dark from dark-on-light, so no OCR assertion is evidence about a colour whatever its page says. A colour smoke decodes its own `grim` capture with `cairo::ImageSurface::create_from_png`.

Its sample band must be derived from the pane-area constants rather than hardcoded, and must be paired with a control band in the counterpart pane. Without the control, a coincidence between the editor's own background and the theme's would satisfy the assertion.

**No pointer smoke hardcodes a pixel inside a pane.** The cell size is measured through the same metrics path, font size and line height the app is launched with, one probe click establishes the origin, and every later target is expressed as a delta from it. The header bar's [toggle group](#sidebar_togglesrs) and the search panel's [toggles](#search_togglesrs) are the exceptions, clicked at targets built from libadwaita's metrics, which their smokes' entries say is safe there. The derived pane origin is checked as a loud, self-checking precondition whose failure message names the candidate causes in order.

What a green run establishes is band membership, not equality: the derived origin need only land within `PANE_ORIGIN_TOLERANCE_PX` plus one cell, a band wider than a cell, so it cannot tell a stale constant from a current one. `PANE_ORIGIN_X_EXPECTED` and `PANE_ORIGIN_Y_EXPECTED` are bit-identical to an origin derived from a solo pointer-smoke run: its persisted probe readback from the smoke's temp working directory, crossed with a cell measured through the same `CellMetrics::for_font` path the app uses. Any change that could move a pane's chrome owes that derivation by hand, not a green tick.

**Two origins exist and neither may be derived from the other.** A probe click reports a Neovim cell, so what it establishes is the text grid's corner; the crash overlay is anchored to the pane area's corner, which is measured its own way and carries its own provenance. The per-pane chrome is the gap between them, and a click derived from the wrong one lands a whole statusbar strip and padding low — outside the button it was aimed at. Re-deriving the geometry after the run does not catch that: the re-derivation recomputes from the same anchor and moves with it, so both sides stay self-consistent while the click misses, which is the vacuity shape the red-run paragraph above collects.

A divider handle is pressed at a modelled position when it belongs to a single `gtk::Paned` whose span is known: anchored at the pane area's corner, `OVERLAY_ORIGIN_X_EXPECTED` or `OVERLAY_ORIGIN_Y_EXPECTED`, with each child floored at `editor_inset::min_pane_size` and the handle at a known fraction of the span. `zoom_pane.rs`'s `tree_handle_x` and `terminal.rs`'s `drawer_strip_edges`, which `drawer_divider.rs` uses, model it so. The press lands because GTK's grab area reaches several pixels either side of the one-pixel separator Adwaita draws. Such a smoke's drag precondition names a missed press as the first cause in its failure message.

The [`pointer.rs`](#pointerrs) divider smokes bisect instead. Their model starts from `PANE_ORIGIN_X_EXPECTED`, the text grid's corner rather than the pane area's, and cannot see each handle's width along a chain of dividers, as the comment above `BISECT_SEED_MARGIN_PX` says; so the positions it derives are seeds only, and the smoke bisects clicks against the window Neovim reports as current. Before each probe it clicks at the current low bound, because a click that lands on the handle changes no window: without that reset the outcome reads as whichever pane the previous probe focused, and the search converges on the wrong side.

**A smoke resizes the window only by resizing the output, and injects no pointer event after it.** `cage` maximizes every parentless toplevel to the output layout, on map and on every layout change, so nothing gives the window a size of its own; `main.rs`'s `RESIZE_OUTPUT_FRAGMENT` sets a custom mode on the headless output with `wlr-randr` and waits for the listing to call it current. Every pixel model above, and the pointer helper's extents, hold at the launch size only, so a press after a resize lands on whatever is there instead.

**Under `ext_multigrid`, a width readback must use `winwidth()`.** Resizing a grid changes the window's grid width, which `winwidth()` reports, and leaves its layout width, which `nvim_win_get_width()` reports, untouched. A readback built on the latter is blind to a divider drag, and it drifts on its own across a relaunch.

**Where the smokes' durable state and configuration go is asserted, not assumed.** The environment helper redirects both directories, and the relaunch smoke checks each in the same four parts: that the test process has no such variable of its own, that the smoke's own directory holds the file, that neither home-derived fallback directory does, and that the developer's real file is byte-identical across the run — including the normal case where it does not exist and must still not exist afterwards, which carries the most weight for the config, since `load_or_create` creates the file it does not find.

**Every smoke is given its own `WOLF_CONFIG_DIR`, and it is the loudest of those redirects.** Without it a smoke would have a `wolf-config.yaml` written for it in the smoke home, carrying a family the pinned fontconfig resolves, and every glyph in every smoke would change at once; `smoke_config_dir` says why, and why the seeded file pins what it does. The directory is a sibling of the working directory rather than a child, for the same reason the state directory is. The one smoke that must let the config decide the font size removes `WOLF_FONT_PT` from its own command after the helper has built it.

**The seeded `appearance` is what makes every OCR page host-independent, and re-pinning it to `auto` would undo that.** Every smoke forces its appearance, so the resolved answer is the forced one whatever the desktop reads, and the chrome flip holds from a dark and from a light starting scheme. An `auto` pin would hand every OCR page back to whatever colour scheme the machine running them happens to prefer.

**The seeded config carries no `gtk-titlebar-style`, so every smoke runs under the default `tabs` unless it pins `native`.** Under `tabs` a one-tab window is unchanged, and a window of two tabs or more keeps the one-tab pane origin, `PANE_ORIGIN_Y_EXPECTED`, which [`titlebar_tabs.rs`](#titlebar_tabsrs) checks. Under `native` a two-tab smoke's pane area starts at `OVERLAY_ORIGIN_Y_EXPECTED + TAB_BAR_HEIGHT_EXPECTED`.

A smoke pins `native` through `main.rs`'s `titlebar_pinned_config_dir` when, with two tabs or more open, it clicks or reads the tab bar's row, or asserts a size or position that includes it, because `tabs` moves that row into the header bar. A multi-tab smoke that asserts on readbacks and saved state alone keeps `tabs`.

## Pointer injection

Real pointer events reach the GTK controllers through [`wolf-pointer-inject`](../../../../tools/wolf-pointer-inject/README.md), this repo's own long-lived virtual-pointer client, built by `flake.nix`. Its README owns the reasons it must be long-lived, why the off-the-shelf tools cannot work, and its command grammar; do not restate them here.

Every smoke that injects a pointer event uses it, among them the crash-overlay one, which needs a real click on the restart button, the preview-link one, whose click is the only injected point aimed at something WebKit drew, and the native-tabs one, whose middle-click is the only injected button-2 press. Each starts the helper inside `cage` before the app, so the seat already carries pointer capability when GTK binds, waits for its readiness line, and then drives it over a FIFO with a poll-for-ack protocol: an ack means the compositor has processed the event, not that the bytes left a buffer.

The scenarios hold the FIFO's write end open on a dedicated descriptor for their whole lifetime, since each write would otherwise close it and hand the helper a spurious end of file, and a trapped cleanup kills the helper before the app so shutdown never races an event.

**In-gesture commands are paced, and the pacing is load-bearing.** Neovim's mouse API transports only the button code through the typeahead while the position rides on globals the handler overwrites, so three calls landing a fraction of a millisecond apart all resolve at the last position. Pacing also stops GDK's motion compression folding intermediate waypoints into one drag update. If a drag smoke flakes, raise the pacing interval; never touch an assertion.

One diagnostic trap is worth writing down: libwayland's client-side debug output prints `interface#id.event` with a `#`, not `@`, so a grep for the wrong separator matches nothing and reads exactly like "no events were delivered".
