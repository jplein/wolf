# `crates/wolf-gnome/tests/smoke_headless/panes.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Split pane smokes

The split smokes name their files unlike the heading words they assert on (`pleft.md` holds `# PANELEFT`), because the tree sidebar renders every cwd file by name and a row reading `paneleft.md` would satisfy the assertion with `<C-w>v` splitting nothing. `PICKER_FILE_NAME` documents the incident behind the rule.

OCR width and height: every pane is opened at `WOLF_FONT_PT=28`, and two or three panes side by side or stacked put the glyphs near tesseract's legibility floor. If OCR misses on size alone rather than a real rendering bug, a larger window or a smaller font for that one test are the knobs; the assertions stay loose `contains` checks. Diagnose from the retained PNG first.

Cmdline visibility: the typed `:e <file><CR>` lands through grid 1, the bottom command strip, so a failure where `:e` opens nothing points at that strip or at `nvim_input` not reaching the cmdline.

The three-pane smoke guards the n-ary frame model: a binary always-nest model collapses the existing right pane B to near zero width. `PaneTree::split`'s same-orientation flatten and `pane_view::install_paned_position`, which re-applies the proportional divider on every `max-position` notify, keep the three columns `[A, C, B]` equal, so `tribtwo` is the assertion that matters.

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

## Pane edge reference colour

The colour comes from the colorscheme through highlight 0, so it is read from the right pane's padding gutter rather than named as a constant, which would pin a palette this smoke takes no position on.

The row is the probe's vertical midpoint, 213 at the pinned output, below the pane's statusbar strip, which is about 92 px tall at `SMOKE_FONT_PT` (two `STATUSBAR_LINE_FACTOR` line boxes, two paddings and a gap). `draw_pane` clips the strip vertically but not horizontally, so a long secondary line can reach the gutter. A reference taken off a glyph reddens the precondition rather than passing quietly, so check that first.

## Per-pane statusbar reverts

Assertion 5's revert: in `draw_pane`, look the cached status up by `self.active_pane.get()` instead of by the `pane_id` being painted. Both strips then draw `statusbravo.md`, so assertion 5 goes red while 6 stays green, which makes it a test of per-pane-ness rather than presence.

Assertion 7's revert: delete the `- 2 * padding` term from `editor_inset::grid_rect`'s `h` (the pane keeps one more row) or the `statusbar_h` term (it keeps several). Either leaves every OCR assertion green, because the inset and the strip height cost whole rows and only a row count reads them exactly.

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
