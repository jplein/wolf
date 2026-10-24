# `crates/wolf-gnome/tests/smoke_headless.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Picker navigation row order

Both navigation files are fresh, so neither is in the MRU cache and `mru_group`/`mru_rank` tie. For the query `pick`, `pick1.md` and `pick2.md` score 124 each (+100 path component prefix, +20 word prefix, +4 contiguous), tie on `match_start`, span and display length, and separate only on the lexicographic `display` tiebreak, so `pick1.md` is row 0.

One `Down` therefore selects `pick2.md`, whose heading `# PICKTWO` OCR must find, and `pickone` must be absent. The smoke relies on that tiebreak order and on `Down` moving the list selection rather than the search entry's text cursor; the `picktwo` assertion catches either assumption breaking.

## Split pane smokes

The split smokes name their files unlike the heading words they assert on (`pleft.md` holds `# PANELEFT`), because the tree sidebar renders every cwd file by name and a row reading `paneleft.md` would satisfy the assertion with `<C-w>v` splitting nothing. `PICKER_FILE_NAME` documents the incident behind the rule.

OCR width and height: every pane is opened at `WOLF_FONT_PT=28`, and two or three panes side by side or stacked put the glyphs near tesseract's legibility floor. If OCR misses on size alone rather than a real rendering bug, a larger window or a smaller font for that one test are the knobs; the assertions stay loose `contains` checks. Diagnose from the retained PNG first.

Cmdline visibility: the typed `:e <file><CR>` lands through grid 1, the bottom command strip, so a failure where `:e` opens nothing points at that strip or at `nvim_input` not reaching the cmdline.

The three-pane smoke guards the n-ary frame model: a binary always-nest model collapses the existing right pane B to near zero width. `PaneTree::split`'s same-orientation flatten and `pane_view::install_paned_position`, which re-applies the proportional divider on every `max-position` notify, keep the three columns `[A, C, B]` equal, so `tribtwo` is the assertion that matters.

## Empty state smoke

`workspace::launch_decision` returns `EmptyState` when the process cwd is the home directory, the desktop-launcher shape, so the smoke sets `HOME` to its cwd and fires the production rule with no test-only variable. `cage_command` sets a default `HOME` of the cwd's parent, under which the app would open a `Folder`, so the override is chained after it.

The `gtk::FileDialog` the button opens has no portal backend under cage, so the smoke covers the empty state's presentation only. It writes no fixture files: the sidebar is hidden, and the asserted `adw::StatusPage` text is large dark-on-light theme text that needs no filler density.

## Palette smoke query order

`PALETTE_FILE` is `pbase.md` while the term asserted absent is the heading word `palcmdbase`, so neither the sidebar row nor the `:e` toast can keep that assertion green once the buffer is replaced. `restore_fixture_names_cannot_satisfy_their_own_assertions` re-checks this in both directions.

No query narrows the list to one row: `score_entry` matches a subsequence of the label and `New` is a prefix of `New Window`, and both commands are dispatched, so both rows stay. `rank_matches` sorts on `(mru_group, mru_rank, Reverse(score), match_start, span, len, display)`, and for `new` the rows separate at `len`, so New Buffer is preselected.

A command MRU holding `new_window` inverts that order and Enter opens a second window. The smoke is safe only because its per-run `WOLF_STATE_DIR` leaves `command-mru.sqlite` empty. `palette::tests::palette_rows_query_new_ranks_the_new_buffer_row_first` pins both the ranking and the inversion, since a screenshot cannot count or order palette rows.

## Buffer picker fixtures

Two smokes share these fixtures and assert opposite things. The query smoke asserts the target's heading present and the preselected row's absent; the bare-Enter smoke asserts the reverse, plus the current file's heading absent. A green bare-Enter run proves `BUFPICK_OCR_ABSENT` is producible on the page, and a green query run with the same cwd proves no sidebar row, `:e` echo or header segment produces it alone.

`buffer_picker::initial_selection` preselects row 1, the most recent other buffer, so with two files the target is the preselected row and a picker ignoring the query still passes. The third file pushes the target to index 2 or later. Each scenario reads the real `getbufinfo()` order back (see `BUFPICK_READBACK_NAME`), and `buffer_picker::tests::buffer_rows_for_the_smoke_query_select_exactly_the_target` pins the arithmetic.

The files are `bp*` and the asserted words `bufpick*`, so the query matches a name while the assertion reads content. The four names are registered once each in `RELAUNCH_FIXTURE_NAMES`, since a duplicate trips its distinct-names check, and the three terms in `ASSERTED_OCR_TERMS`.

## Project search fixtures

A result row is a location, so the editor must land on the matching line and column. A screenshot only shows that the file is on screen, which an `:edit` with no cursor placement also satisfies, so the captive Neovim writes `expand('%:t')`, `line('.')` and `col('.')` in one `printf` and OCR only corroborates.

The files are `psr*` and the asserted words `projsearch*`. `PROJSEARCH_QUERY` is a third vocabulary: content of the target file, drawn once the jump lands, so it must not contain any `ASSERTED_OCR_TERMS` entry, and it must appear on exactly one line of one file so the activated row is deterministic. `restore_fixture_names_cannot_satisfy_their_own_assertions` asserts both.

## Relaunch smoke exit codes

The relaunch smokes run two sequential app launches inside one `cage`, so each holds one cage slot. Three share the codes below; the divider smoke keeps `POINTER_PREAMBLE`'s codes for the helpers it copies and carries its own table.

- `11`: run 1's app died before it was ready for input.
- `12`: run 1 was still alive after the quit poll budget, so the drain and close path failed.
- `13`: run 1 exited non-zero. That implicates a panic in `Coordinator::on_window_closing` (gather, commit and blocking flush) or a `RefCell` double borrow against `window::wire_close`'s `core.borrow_mut().take()`.
- `14`: run 2's app died before it was ready for input.
- `15`: the Neovim readback file never appeared in run 2.
- `16`: run 1 wrote no `state.json` (the single-pane and two-window smokes).
- `21` to `26`: the two-window smoke's own, in `TWOWIN_SCENARIO`'s table.

Every observable is polled rather than slept for. Run 1 is reaped with `wait` so its exit status is checked, because "the process is gone" is equally true of a clean shutdown and of a panic in the GTK main loop.

## Two window smoke fixtures

`cage(1)` shows a single maximized application, and presenting a backgrounded `adw::ApplicationWindow` under cage does not raise it, so no capture can show two windows. The two observables are the count of live `nvim` children of the app pid (the crash smokes' `/proc` scan) and the `state.json` run 1 wrote, frozen before run 2 can overwrite it.

The second window opens from inside the one process, through the palette (Ctrl+Shift+P, `TWOWIN_NEW_WINDOW_QUERY`, Enter). A second `wolf-gnome` launched under the same cage hands off to the first through single-instance activation on cage's private `dbus-run-session` bus, which exercises the launch path instead of New Window.

Window B is `:cd`'d into a subdirectory and opens a file there. The per-project swap directory is keyed on the cwd and outlives the process, so two windows on one project meet Neovim's correct `W325: Ignoring swapfile` warning, which is not this smoke's subject. Two projects also make the two saved `cwd` values distinct, so "restored in saved order" is falsifiable.

The fixture names may be the strings the assertions compare, because no page is read: the strings come from `state.json`'s `TabState::layout` leaves and from the captive Neovim's own `getcwd()`, which no sidebar row or toast can write. The names are still in `RELAUNCH_FIXTURE_NAMES`, and the exemption block of `restore_fixture_names_cannot_satisfy_their_own_assertions` states the exemption and the crossing executably.

## Open Recent fixtures

The scenario is the palette scenario plus the buffer picker's `wait_file` helper: open a base file, Ctrl+Shift+P, type the target directory's basename, Enter, then read `getcwd()` back. `make_temp_cwd` returns `<unique>/project` and `cage_command` passes `<unique>` as `HOME`, so the target `<unique>/orzulu` is drawn as the constant `~/orzulu`. A target directly under `std::env::temp_dir()` draws a run-varying path in the header and brings back the OCR flake `make_temp_cwd` describes.

`SIDEBAR_FILLER_FILES` go into both directories, because the sidebar re-roots and the target's rows are the only ones on the asserted page. The subject is `ortango.md`'s name, the sidebar row, so that file is the one deliberate exception to "put the asserted word in the content", and like `TREE_FILE_NAME` it is absent from `RELAUNCH_FIXTURE_NAMES`.

The header draws `~/orzulu` on every capture, so a target directory named after the asserted term would pass from the title alone. The third fixture guard's exemption block asserts both halves. Assertion 5 reads the sidebar because only it proves `settle_reroot` ran `tree_view::reroot`; the title proves no more than assertion 3's readback.

There is no must-not-contain assertion: `InputEvent::Chdir` is `:tcd`, which closes no buffer, so the earlier buffer and `:e` toast may still be painted.

## Navigation history smoke

This covers the whole per-pane navigation path: the capture-phase chord controller resolving `<Alt>Left` through `actions::action_for_key` to `win.history-back`, installed from `command_action::ui_action_for_front_end_action` because it has no `wolf_core::Command`; `shell::perform`'s `UiAction::HistoryBack` arm; `nav_ui::nav_press`; `WolfCore::history_back` moving the pane through `_G.wolf_history_set_buf`; and the repaint.

Both fixture names are unlike their asserted words, the `PICKER_FILE_NAME` rule, which `restore_fixture_names_cannot_satisfy_their_own_assertions` checks. OCR alone cannot tell "Back worked" from "the second `:e` never happened", since both leave the first file's heading on screen, so the captive Neovim writes `bufname('%')` before the chord as a precondition and after it as the discriminator.

## Sidebar toggle smoke

This is the only end-to-end coverage of `win.toggle-sidebar`: the chord through `actions::action_for_key`, `command_action::ui_action_for` classifying `Command::ToggleSidebar` as a dispatched `UiAction::ToggleSidebar`, `shell::perform` flipping the preference, and `AppShell::apply_sidebar_visibility` as the one site that touches the widget.

The sidebar's only text is file names, so the observable is a name that vanishes. `SIDEBARTOGGLE_FILE` is named after its term, the exemption `TREE_FILE_NAME` and `OPENRECENT_TARGET_FILE` carry, and is never opened, so only a sidebar row draws it. The fixture guard asserts the exemption's preconditions.

A must-not-contain is satisfied by a crashed window, a blank one and a sidebar that never populated. Capture A must contain the term, capture B must contain the keep term, and the guard asserts the crossed fixture pair. Each of the three closes one of those, so none can be weakened.

## Typed cd fixtures

One scenario carries both the synthetic cwd root node and the re-root on Neovim's own working directory. A standalone root-node smoke has no discriminating assertion: the root row draws the cwd's basename and the header draws `~/<basename>` on every capture. Only `cwd == home` makes `title::display_title` draw a bare `~`, and `workspace::launch_decision` refuses `home` as a launch project, so a live `:cd` is the only route; a `grim -g` region capture would need hardcoded geometry.

```text
<unique>/project/            base (make_temp_cwd, used only as a base)
  cdouterhome/               CD_OUTER_DIR   -- drawn only if the ~ abbreviation fails
    cdrootzulu/              CD_ROOT_DIR    -- the app's HOME and the :cd target
      cdtango.md             CD_TARGET_FILE -- asserted present (a sidebar row)
      fill*.md               nine fillers, post-re-root page density
      startdir/              CD_START_DIR   -- the launch cwd
        cdbase.md            CD_BASE_FILE (# ZZCDBASE) + nine fillers
  cdstate/                   CD_STATE_DIR -- the WOLF_STATE_DIR override
  cdcwd.txt                  CD_READBACK_NAME
```

Fact 1: `cage_command`'s default `HOME` is `smoke_home(cwd)`, the cwd's parent, so the app's `HOME` is `cdrootzulu` with no override, and after `:cd ..` the title reads `~` and `cdrootzulu` can reach the page only from a sidebar row.

Fact 2: `WOLF_STATE_DIR` must be overridden, chained after `cage_command`. The default `cdrootzulu/startdir-state` sits inside the post-`:cd` search root, and Neovim's swap files there encode the buffer path with `%` separators, so a swap-file row would contain `cdrootzulu` and satisfy assertion 6. The empty directory `cage_command` still creates yields no row, because `TreeIndex::build` derives directories from file entries only.

Fact 3: the readback lands in `base`, outside both search roots, and its absolute path in the typed `writefile` line carries no asserted term.

The `:cd` is relative because the typed command line is page text and a toast can outlive it, so an absolute `:cd` would put `cdrootzulu` on the page with nothing re-rooted.

## Per-pane statusbar fixtures

The asserted terms are file names, inverting the usual fixture rule, because a per-pane status bar's claim is that the strip above each pane carries that pane's file. Three other routes could draw a name, and each is closed by construction.

The tree sidebar lists every cwd entry, so the fixtures sit in `<unique>/sbfixtures/`, under `HOME` but outside the search root, and are opened by absolute path. The cmdline strip echoes the last `:e` path, so one more Ex command, an `:echo` of `STATUS_OCR_MARKER`, runs before the capture. Neovim's own statusline is off through `laststatus = 0` in `runtime/nvim/init.lua`, and the readback asks the captive Neovim for `&laststatus` so assertion 3 pins it at 0.

```text
<unique>/                    the app's HOME (cage_command's default)
  project/                   TEMP_CWD_BASENAME -- the launch cwd and search root
    fill*.md                 nine fillers, page density
  sbfixtures/                STATUS_FIXTURE_DIR -- outside the search root
    statusalfa.md            STATUS_FILE_A, opened in the first pane
    statusbravo.md           STATUS_FILE_B, opened in the split pane
  status_winheight.txt       STATUS_READBACK_NAME -- outside the search root too
```

## Markdown preview fixtures

The scenario opens the other file, splits with `<C-w>v`, opens the Markdown source in the new pane and fires Preview there. `wolf_open_preview` binds in place, replacing the source pane's buffer, so the source's raw text is in no pane. Neovim cannot draw `PREVIEW_OCR_BODY` and neither file name contains it, so an OCR hit can only be WebKit's pixels. A placement that split a third pane would leave the source visible, which is why the persisted layout must hold two leaves and the readback two windows.

The live edit runs after the Edit leg because only a second load into a live web view takes the JSON body-swap branch (`pane_view::preview_swap_payload`); the first goes through `load_html` whatever the encoding does. A single-encoded payload prints `JSON Parse error: Unrecognized token '<'` on every keystroke while the preview keeps the stale document.

Editing needs the source in a window, and Edit in Another Pane puts it in a split, the leg's own discriminator; the in-place Edit replaces the preview it is fired from. The marker is typed there, so the other file is opened over it before capture B, leaving only the preview to draw it. A live edit before the Edit leg needs a `:e` of the source that would satisfy that leg's readback alone.

Neither file name contains a heading term, and both names and the two readbacks are in `RELAUNCH_FIXTURE_NAMES`. `PREVIEW_OCR_STATUS` is an ordinary word, so the fixture guard also checks that nothing this smoke puts on screen spells it.

## Preview link click fixtures

The one pointer smoke whose click target is something WebKit drew. It shares `POINTER_PREAMBLE` with the other `pointer_scenario` bodies.

The cwd is not the directory the link resolves in. With `load_html(html, None)`, WebKitGTK hands the handler the raw relative href (`uri="lnkinto.md"`), `preview_link::classify` takes its schemeless row, and `WolfCore::preview_file` passes the relative path to Lua's `filereadable`, which resolves it against the captive Neovim's working directory. With the fixtures at the project root that is the source's directory, so the follow succeeds by coincidence and the base URI is invisible. `LINK_FIXTURE_DIR` separates the two answers.

No fixture path contains an `ASSERTED_OCR_TERMS` entry, `preview` included. The registered names carry the `LINK_FIXTURE_DIR` prefix, so the guard's substring loop covers the directory with no exemption block. The smoke's observables are raw pixels in the preview pane and a `tabpagebuflist()` readback, which no sidebar row can move.

## Pane edge reference colour

The colour comes from the colorscheme through highlight 0, so it is read from the right pane's padding gutter rather than named as a constant, which would pin a palette this smoke takes no position on.

The row is the probe's vertical midpoint, 213 at the pinned output, below the pane's statusbar strip, which is about 92 px tall at `SMOKE_FONT_PT` (two `STATUSBAR_LINE_FACTOR` line boxes, two paddings and a gap). `draw_pane` clips the strip vertically but not horizontally, so a long secondary line can reach the gutter. A reference taken off a glyph reddens the precondition rather than passing quietly, so check that first.

## Fire scope arithmetic

Assertions 12 and 13 read `prev.render`, whose rows exist whether or not the sample sat in a `pane.prev` region, so deleting the `frame_metrics::scope(Bucket::PanePrev)` guard from the fire closure leaves them unchanged and makes `frame_stats.rs`'s nesting table false.

A pass is opened by every pane wake that reaches it plus every performing coalescer fire, so `count(pane.prev)` is `count(pane.wake)` minus the wakes that return before the pass, plus the fires. The subtracted term is zero here: with the fire's scope deleted the counts are equal (299 against 299). So the comparison is strict, since a `>=` passes the reverted build.

The strict margin is the performing fires, +4 on three green runs (298/294, 303/299, 309/305) against +0 with the scope deleted. Comparing against `prev.render` proves nothing, because the pass runs on nearly every wake and 298 > 7 holds even with the coalescer removed.

## Restart button click point

`gtk::Widget::set_size_request` sets a minimum and the button is `halign(Center)`, so a builder requesting too little still renders a natural-width pill centred on the rect's centre. A builder requesting `RESTART_BUTTON_WIDTH` (320) renders x 605..924; one requesting 40 renders x 669..861 (193 px). Both are centred on x 765, so a centre click hits both, and even the 3/4 point (x 845) hits the small one.

A tenth in from the left edge puts the click at x 638: 33 px inside the correct button and 31 px clear of the natural-width one. The rect agrees with the render to within half a pixel (x 605..925, y 392..456 against a derived x 605.5..925.5, y 391.5..455.5), and the anchor is the overlay's own measured origin, so the derivation error is far below either margin.

As a bound: the button spans `[765 - W/2, 765 + W/2]` with `W = max(requested, natural ~193)`, and a click at x 638 misses it if and only if `W < 254`. It catches a builder width below about 254 px, the 40 px revert included, but widths between 254 and 320 go undiscriminated.

Y stays at the rect's vertical centre. The vertical derivation, two `vexpand` spacers splitting the leftover, is the harder half and is most robust there, and no documented revert turns on the button's height, the other axis this click leaves undiscriminated.

## State dir isolation checks

Check 1, the positive one: run 1's `state.json` exists and is non-empty at the smoke's own `WOLF_STATE_DIR`. Drop the variable and the file is absent (the scenario exits 16 first).

Check 2: `cage_command` redirects the app's `HOME` into the temp tree, so with `XDG_STATE_HOME` unset an app ignoring `WOLF_STATE_DIR` resolves `state_dir()` to `$HOME/.local/state/wolf`, inside the temp tree where check 3 cannot see it. Those exact paths must hold no `state.json`, which positively proves the variable was honoured. The path comes from the same `smoke_home` helper `cage_command` uses, so the check and the environment cannot drift.

Check 3 compares the developer's real state file. It bites on a host that exports `XDG_STATE_HOME` or `XDG_DATA_HOME`, where a leak lands outside the temp tree and check 2 is blind, so checks 2 and 3 together cover both environments.

## Picker over preview geometry

`picker.rs`'s `PICKER_BOX_MARGIN`, `PICKER_BOX_WIDTH` and `PICKER_BOX_HEIGHT` are private, and an integration test is an external crate. Making them `pub` for this file would be circular: a box that moved together with its constants would keep every assertion green. So the box is located from the pixels on both axes, its surface colour is read from the capture, and nothing in `src/` changes for this smoke.

`pickover_fill_reference_band` is four absolute pixel literals inside a region whose extent this file may not import. The smoke asserts the two properties they exist for, uniform with the picker open and absent with it closed, so a band that leaves the box fails loudly instead of reading another widget's colour.

## Picker fill colour

Uniformity says the band sits inside one flat surface rather than straddling an edge, which licenses reading the colour off one pixel. Absence from the same band in the control says the surface is the picker's, not the editor pane's; without it, a run where the box never appeared would read the editor's background as the fill and pass assertion 8.

Three inequalities say what the colour may not be. The first two keep assertion 8's three counts distinguishable, and the third, against `PICKOVER_UNSTYLED_FILL`, is the only reading that tells the overlay chrome apart from the mere presence of an opaque box.

## Picker selected row scan

Assertion 4's `PICKOVER_UNSTYLED_FILL` inequality reads the box's surface, and nothing else reads a row metric, so `padding: 8px 10px` on `list > row` could be deleted with every other assertion green. The box is 600x400 whatever the rows do, so the selection highlight, exactly one row tall, is what is measured.

The scan assumes one row in the box. `PICKOVER_QUERY` is a prefix of `PICKOVER_SOURCE_FILE` and of nothing else in the cwd, so the list narrows to one row above empty fill; with more rows the empty fill the walk starts from moves off the reference band.

The walk goes up from the reference band, which assertion 4 proved is the box's fill, and stops at the first row that is neither fill nor selection. The search entry above the list is neither colour, so the walk ends there or at the box's top edge, and an accent in the tab bar or header can never be counted as a list row.

## Typed cd reverts

Assertion 4's revert: at the top of `shell::settle_reroot`, add `if self.inner.pending_reroot.borrow().is_none() { return; }`, restoring "only a front-end chdir re-roots". Assertion 3 stays green, the sidebar keeps listing `startdir`'s files and `cdtango` never reaches the page. The revert also makes assertion 6 unsatisfiable, but 4 fails first.

Assertion 6's revert: in `tree_view`'s root-store rebuild, append `tree::children_of(&entries, "")` instead of the single `root_node(..)`. Assertion 4 stays green because the children still render at top level, so the two rows are independently falsifiable despite sharing one scenario.

## Per-pane statusbar reverts

Assertion 5's revert: in `draw_pane`, look the cached status up by `self.active_pane.get()` instead of by the `pane_id` being painted. Both strips then draw `statusbravo.md`, so assertion 5 goes red while 6 stays green, which makes it a test of per-pane-ness rather than presence.

Assertion 7's revert: delete the `- 2 * padding` term from `editor_inset::grid_rect`'s `h` (the pane keeps one more row) or the `statusbar_h` term (it keeps several). Either leaves every OCR assertion green, because the inset and the strip height cost whole rows and only a row count reads them exactly.

## Terminal drawer smokes

S1, `terminal_drawer_opens_and_is_not_persisted_under_cage`, is the only evidence the drawer reaches a pixel, plus the persisted-layout leg and six focus readbacks, at `SMOKE_FONT_PT`. S2, `terminal_drawer_defaults_to_full_width_under_cage`, reads three integers at `FULLWIDTH_SMOKE_FONT_PT`, because the content-width cap cannot bind at `SMOKE_FONT_PT`. S3, `terminal_drawer_reopens_after_a_neovim_restart_under_cage`, is the only falsifier for `reset_after_restart` clearing the drawer, which needs a real crash with the drawer bound before the kill.

`wolf_toggle_terminal` ends in `startinsert`, so after the chord every keystroke goes to bash, and an Ex command typed there writes no readback and surfaces as exit 14. Every readback is preceded by `CTRL-\ CTRL-N` (the `to_normal` function), a no-op in Normal mode, so red runs still produce a readback instead of a timeout.

`runtime/nvim/init.lua`'s re-entry autocmd fires `startinsert` on WinEnter into the drawer, so the rule above applies after every focus move. Normal-mode `<C-w>h/j/k/l` are `noremap`ped to a pure intent (`do_focus`), so Neovim never moves focus itself, and every `bt=[…]` readback after a focus chord speaks about the front-end's `apply_focus`.

## Native tabs smoke

`cage` shows one toplevel at a time, so the two-window smoke can photograph nothing, but a tab bar is inside the one toplevel and the two-tab state is photographable.

No assertion reads a tab title. `tabs_ui::tab_title` draws the current file's name, as do the statusbar strip and the `:e` echo, so no OCR term separates them, and tesseract reads one of two rendered titles at this page's calibration. The fixture names carry no asserted term, and every OCR claim is about a heading in an editor grid. `bar.is_visible()` reads `true` at one page as well as two; see `TAB_BAR_HEIGHT_EXPECTED`.

```text
<unique>/                     the app's HOME (cage_command's default)
  project/                    TEMP_CWD_BASENAME -- the launch cwd and search root
    fill*.md                  nine fillers, page density
    tbxray.md                 TABS_FIRST_FILE, opened in tab 1
    tbsub/                    TABS_SUBDIR -- the `:tcd` target
      tbyankee.md             TABS_SECOND_FILE, opened in tab 2
    tb_read{a,c,e,f}.txt      the four Neovim readbacks
```

## Packaged smokes share this file

The packaged smokes exercise the binary `nix build .#wolf-gnome` installs, which is built by a different command from `CARGO_BIN_EXE_wolf-gnome` and is the one a user launches, so the other smokes say nothing about it. The store-layout assertions need no compositor and live in `crates/wolf-gnome/tests/packaged_layout.rs`.

They live here because a second test file would need a compositor spawn of its own. `cage_is_only_spawned_through_the_isolating_helper` exists to forbid that second spawn site, but it reads only this file's source, so it could not see one elsewhere. For the same reason `CAGE_SPAWN_NEEDLE` is assembled from fragments: a contiguous spelling anywhere in this file counts as a second spawn site and turns the guard red.
