# `crates/wolf-gnome/tests/smoke_headless/sidebar.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Project search fixtures

Two smokes share the `psr*` fixtures. The first reads the capture it takes between its two Enters in two bands, the sidebar's and the pane area's. `zzmango`, the last word of the target's match line, is never typed and its file is not open yet, so only a result row can draw it, and the band it lands in says whether the results are in the sidebar or over the editor.

The second Enter opens the match. An `:edit` with no cursor placement shows the same file, so the captive Neovim writes `expand('%:t')`, `line('.')` and `col('.')` in one `printf` and OCR only corroborates.

`psrxray`, the starting file's name, is the tree-only term: inside the sidebar band only the Files page draws it, so its absence says the Search page is up and its presence says the tree is back. That file is exempt from `RELAUNCH_FIXTURE_NAMES`, and the guard crosses it against everything the Search page draws.

The content terms are `projsearch*`. `PROJSEARCH_QUERY` is a third vocabulary: it appears on exactly one line of one file, so the activated row is deterministic, and it contains no `ASSERTED_OCR_TERMS` entry. `restore_fixture_names_cannot_satisfy_their_own_assertions` asserts all of this.

## Open Recent fixtures

The scenario is the palette scenario plus the buffer picker's `wait_file` helper: open a base file, Ctrl+Shift+P, type the target directory's basename, Enter, then read `getcwd()` back. `make_temp_cwd` returns `<unique>/project` and `cage_command` passes `<unique>` as `HOME`, so the target `<unique>/orzulu` is drawn as the constant `~/orzulu`. A target directly under `std::env::temp_dir()` draws a run-varying path in the header and brings back the OCR flake `make_temp_cwd` describes.

`SIDEBAR_FILLER_FILES` go into both directories, because the sidebar re-roots and the target's rows are the only ones on the asserted page. The subject is `ortango.md`'s name, the sidebar row, so that file is the one deliberate exception to "put the asserted word in the content", and like `TREE_FILE_NAME` it is absent from `RELAUNCH_FIXTURE_NAMES`.

The header draws `~/orzulu` on every capture, so a target directory named after the asserted term would pass from the title alone. The third fixture guard's exemption block asserts both halves. Assertion 5 reads the sidebar because only it proves `settle_reroot` ran `tree_view::reroot`; the title proves no more than assertion 3's readback.

There is no must-not-contain assertion: `InputEvent::Chdir` is `:tcd`, which closes no buffer, so the earlier buffer and `:e` toast may still be painted.

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

## Typed cd reverts

Assertion 4's revert: at the top of `shell::settle_reroot`, add `if self.inner.pending_reroot.borrow().is_none() { return; }`, restoring "only a front-end chdir re-roots". Assertion 3 stays green, the sidebar keeps listing `startdir`'s files and `cdtango` never reaches the page. The revert also makes assertion 6 unsatisfiable, but 4 fails first.

Assertion 6's revert: in `tree_view`'s root-store rebuild, append `tree::children_of(&entries, "")` instead of the single `root_node(..)`. Assertion 4 stays green because the children still render at top level, so the two rows are independently falsifiable despite sharing one scenario.
