# `crates/wolf-gnome/tests/smoke_headless/editor.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Picker navigation row order

Both navigation files are fresh, so neither is in the MRU cache and `mru_group`/`mru_rank` tie. For the query `pick`, `pick1.md` and `pick2.md` score 124 each (+100 path component prefix, +20 word prefix, +4 contiguous), tie on `match_start`, span and display length, and separate only on the lexicographic `display` tiebreak, so `pick1.md` is row 0.

One `Down` therefore selects `pick2.md`, whose heading `# PICKTWO` OCR must find, and `pickone` must be absent. The smoke relies on that tiebreak order and on `Down` moving the list selection rather than the search entry's text cursor; the `picktwo` assertion catches either assumption breaking.

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

## Navigation history smoke

This covers the whole per-pane navigation path: the capture-phase chord controller resolving `<Alt>Left` through `actions::action_for_key` to `win.history-back`, installed from `command_action::ui_action_for_front_end_action` because it has no `wolf_core::Command`; `shell::perform`'s `UiAction::HistoryBack` arm; `nav_ui::nav_press`; `WolfCore::history_back` moving the pane through `_G.wolf_history_set_buf`; and the repaint.

Both fixture names are unlike their asserted words, the `PICKER_FILE_NAME` rule, which `restore_fixture_names_cannot_satisfy_their_own_assertions` checks. OCR alone cannot tell "Back worked" from "the second `:e` never happened", since both leave the first file's heading on screen, so the captive Neovim writes `bufname('%')` before the chord as a precondition and after it as the discriminator.
