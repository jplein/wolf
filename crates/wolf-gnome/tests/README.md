# wolf-gnome/tests

Integration tests for [`wolf-gnome`](../README.md). This crate is Cargo-only by design — it has no Bazel target, so there is no `rust_test` wiring to keep in sync, and `cargo test` discovers every file here.

```sh
nix develop --command cargo test -p wolf-gnome
```

The devShell is not optional: it supplies GTK, exports `WOLF_NVIM_BIN` and `WOLF_RG_BIN`, and pins the font stack, which is what makes Pango measurement and Cairo rasterization reproducible enough to assert on pixels.

Rules that are decidable from their arguments are pinned in each seam's own `#[cfg(test)] mod tests`, which is the point of the seam split in [wolf-gnome-decisions.md](../../../notes/wolf-gnome-decisions.md).

Some seams' tables are files here instead: [`drawer_share.rs`](#drawer_sharers), [`gdk_key.rs`](#gdk_keyrs), [`terminal_seams.rs`](#terminal_seamsrs), [`other_pane_seam.rs`](#other_pane_seamrs), [`open_request.rs`](#open_requestrs), [`pane_zoom.rs`](#pane_zoomrs), [`pane_views.rs`](#pane_viewsrs), [`preview_root_style.rs`](#preview_root_stylers), [`sidebar_toggles.rs`](#sidebar_togglesrs), [`titlebar_background.rs`](#titlebar_backgroundrs), [`restore_caps.rs`](#restore_capsrs), [`app_state_seam.rs`](#app_state_seamrs), [`stacking.rs`](#stackingrs), [`tab_bar_slot.rs`](#tab_bar_slotrs), [`tab_focus.rs`](#tab_focusrs) and [`tab_titles.rs`](#tab_titlesrs).

The other files below cover what a seam cannot reach: a real Neovim, a real window, the built binary together with the manifest that shaped it, and the store output `nix build .#wolf-gnome` installs — a different artifact from the checkout build, making different claims.

## Files

#### `about_notices.rs`

**Covers**

- `about::about_text()` carrying each linked library's licence phrase, re-typed locally, and `wolf_core::LIBRARY_LICENSE_NOTICES` verbatim. The font phrases are pinned by [`about.rs`](../src/README.md#aboutrs)'s own tests.

**Gotchas**

- Pure: no display, no Neovim and no compositor.

#### `app_state_seam.rs`

**Covers**

- `state_seam::restore_plans` over a saved `app_state::WindowState`: each tab planned at its own directory, a missing one as an empty tab that keeps its saved tab, the saved active tab and an out-of-range one, each window's order, size and minimized flag, the extra window a terminal launch from an unheld directory puts in front and none from a held one, and a tabless saved window dropped.
- Each plan's sidebar page, search fields and drawer share from its saved tab, a saved share outside `(0, 1)` restoring a new one; `sidebar_entry` and `restored_sidebar` round trips, a missing toggle at its initial value; `restore_runs_search`; and `may_capture_cursor`, `Alive` alone.
- The write side: `window_entry`'s `active_tab`, its fallback to the most recent contributing tab and its recency indices, a frame with no position, and no entry with no tab; `window_state`'s stacking order and minimized flag, and nothing with no window; `tab_entry`, an empty tab writing its sidebar alone and a folder tab its root, a lone empty pane included, zoom, focus, drawer and sidebar; and `seeded_tab` with the live sidebar.
- `present_after`, the restore's present schedule: each window waiting for the one built before it, and a minimized window presented at once, nothing waiting for it.
- [§7](../../../specs/application-state.md#7-new-windows-and-tabs)'s `new_window_size`, stored over active over `None`, and `active_size`, the front window's or `None` with none or an unusable size.

**Gotchas**

- Pure: no display, Neovim or filesystem; `is_dir` answers from a fixed list. Whether the coordinator feeds them the live windows is [`smoke_headless/restore.rs`](#smoke_headlessrestorers)'s question.

#### `cell_text_paint.rs`

**Covers**

- `render::draw_cells` onto an offscreen ARGB32 `ImageSurface` exactly the size of a one-line toast's box: a line of narrow glyphs leaves the right padding without ink and inks its last cell, at a size where the same line laid out as one Pango run overruns the box.
- Its return value, which `draw_toast` advances each chunk by: two cells for a wide character, none for a combining mark, and none for empty text, which draws nothing.

**Gotchas**

- Headless: no display, no Neovim and no compositor, but it needs the dev shell's pinned fontconfig, as [`paint_golden.rs`](#paint_goldenrs) does.
- The font size is chosen by the precondition `a_pango_run_of_the_line_outgrows_the_toast_box`. If a font update makes it fail, choose another size by it; weakening the clip assertions instead leaves a test that passes whether the line is drawn on its cells or as a run.

#### `drawer_share.rs`

**Covers**

- `drawer::DrawerShare`: a new share is `DRAWER_WEIGHT`, the third `init.lua` opens the drawer's split at, and `Default` is `new`; `position` against GTK's clamp at both floors; `drag_to`, a non-positive span leaving the share alone; a drag-then-position round trip at every pixel of three spans; a dragged share keeping its proportion across pane-area resizes, clamped at the floors; and `restored` keeping a saved share only strictly between 0 and 1.

**Gotchas**

- Pure: no display, no Neovim and no compositor. The limits model the smokes' 176 px floor and one-pixel handle, and every expected position is written out by hand.
- Whether the paned reads and writes the share is [`smoke_headless/drawer_divider.rs`](#smoke_headlessdrawer_dividerrs)'s question.

#### `flake_exports.rs`

**Covers**

- That every `export` in `flake.nix`'s `linuxShell.shellHook` is on a packaged smoke's scrub list or on `KEPT_FOR_THE_COMPOSITOR`, and that the hook still exports the vendored-tree variables the `wolf-ffi` tests need. The [`flake.nix` notes](../../../notes/flake.nix.md#shellhook-exports) say which list takes which export.

**Gotchas**

- Pure: it reads only `flake.nix`, so it runs in seconds with no compositor. It is a binary of its own so that a `flake.nix` change can be checked without the smokes, with `cargo test -p wolf-gnome --test flake_exports`.

#### `frame_metrics_gate.rs`

**Covers**

- The disabled half of `frame_metrics`, in a process nothing armed: `enabled()` answers false rather than defaulting true or panicking on an unwritten `OnceLock`, and `begin()` answers `None`, which is the whole "an unset flag costs one boolean test and no clock read" claim.
- That `end`, `scope`, `note_keystroke`, `note_wake`, `maybe_emit` and `flush_final` are total against an absent recorder — called over every bucket, and more than once each, since a second call is what would find state the first left behind.

**Gotchas**

- It needs a test binary of its own, and nothing in it may call `install()`. `enabled()` reads a process-wide `OnceLock`, so whichever test reaches `install()` first fixes the answer for every test after it in that binary, in any order and on any thread. Folded into an existing file these cases would stay green while proving nothing.
- The armed half belongs to a different process and is the frame-stats smoke's. Arming it here would mean an `unsafe` `std::env::set_var` racing the harness's other threads, and would burn the `OnceLock` for the rest of the binary.
- It cannot say that nothing reached stderr: capturing file descriptor 2 needs a dev-dependency this crate declines, for the lock-file reason [`release_profile.rs`](#release_profilers) records.

#### `gdk_key.rs`

**Covers**

- Table-driven `gdk_key(keyval, state) -> Option<KeyEvent>`: named keys, arrows, F-keys, the printable shift contract (`shift` is always false for `Key::Char`; the keyval carries the case), modifiers on both named and printable keys, and bare modifier/lock keys yielding `None`. Runs with no display and no nvim.

**Gotchas**

- Assert only on letter casing and named keys. Shifted-punctuation unicode (that Shift+`,` yields `<`) is keyboard-layout dependent and would be flaky.

#### `im_commit_nvim.rs`

**Covers**

- The commit rulebook against a real Neovim — `im_ui::commit_decision` to `InputEvent::Key` to `WolfCore` to snapshot: a composed character arrives as itself, a multi-character commit whole and in order, a control character inside a commit not at all, and a commit offered with an overlay open sends nothing.
- Two premises about the real Neovim a table could not state: a raw ESC through `nvim_input` ends insert mode, so the character behind it becomes a motion and the row reads `a` rather than `ab`; and `dd` delivered as two `Key` events deletes a line where the same text as a `Paste` would insert it.

**Gotchas**

- Requires `WOLF_NVIM_BIN`; `common::require_nvim_bin` panics rather than skipping. No display and no `gtk::IMContext`: `send_commit` is `wire_keys`' commit handler with the GTK removed, so what the cases pin is the rulebook and the transport rather than the widget wiring — `composed_text_reaches_neovim_under_cage` joins those.
- The refusal case commits while still in insert mode, deliberately: in normal mode a leaked `é` changes no buffer, so the assertion would hold with the guard deleted.
- The normal-mode case pins the transport, not the seam: `im_ui` answers `Send` either way, and the choice of `InputEvent::Key` over `InputEvent::Paste` is the test loop's own.
- Leaving insert mode emits follow-up flushes, so every case drains, sleeps a settle window and drains again.

#### `mouse_nvim.rs`

**Covers**

- The pure pointer seams driven into a real `WolfCore` and a real Neovim, doing by hand what a gesture or scroll callback would do: `cell_at` for the cell, `mouse_input` for the button and modifiers, the accumulator and `scroll_steps` for the deltas, then `send_mouse` and `send_scroll`. A press, drag, and release select a range; the drag point is asserted to clamp to the last cell before it is sent. A sub-cell scroll carry crosses exactly once, a positive delta strictly increases the viewport's top line, and a negative one strictly decreases it.
- The refusal of a non-primary button, so no context-menu click can reach Neovim.

**Gotchas**

- No GTK controller is involved, so this file cannot say whether a controller ever fires. That is the smokes' question, and the two deliberately do not overlap.
- Cell sizes are locally declared rather than measured, because the seams are pure arithmetic and need no font.
- The buffer fixture's lines must stay five characters wide, or a click at column 2 clamps to end-of-line and the exact-column assertion means nothing. A literal double quote cannot appear inside a `:put =` expression, which is why the format string is quoted with doubled single quotes.

#### `open_folder.rs`

**Covers**

- `open_folder::step`, `plan` and `fill_size` against [§6](../../../specs/application-state.md#6-opening-a-folder), in the cases [`OpenFolderSeamTests.swift`](../../wolf-macos/Tests/WolfKeyMapTests/README.md#openfolderseamtestsswift) runs: a background tab of a back window focused; the frontmost of two windows whose selected tab is on the directory; the more recent of two background tabs; an empty current tab filled; a current tab with a directory opening a window.
- A project state planned as one folder tab with its saved tab, sidebar, fields, drawer share and size; none planned as a fresh folder tab with no size; a fill's size applied to a lone tab only.
- `plan`'s `new_tab_sidebar`: with no project state, a captured hidden sidebar planned hidden and nothing captured planned shown; a project state's stored setting winning over a captured one.

**Gotchas**

- Pure: no display, no Neovim and no filesystem. No window or tab id equals an index asserted on, and every window list is passed out of stacking order, so only the stacking puts the answer right.
- The opened directory exists on no host, so a plan that consulted the filesystem would plan an empty tab.

#### `open_request.rs`

**Covers**

- `open_request::route` and `current_tab` against [§3 to §5.1](../../../specs/cli-and-file-open.md#3-tab-ids), in the cases of [`FileOpenSeamTests.swift`](../../wolf-macos/Tests/WolfKeyMapTests/README.md#fileopenseamtestsswift) a pure seam can hold: the frontmost window's selected tab, else its most recent; a live `WOLF_TAB_ID` naming a background tab as a directory's current tab, and a file's tab whatever its project; an unmapped ID, and one mapped to a tab no window holds, handled as unset; a file without one going to the deepest open project.
- `placement` resolving each index to its leaf, and `None` past the last; `may_place` admitting `Alive` alone; `PaneTree::split_root_left` over a leaf, a `Vertical` and a `Horizontal` root.

**Gotchas**

- Pure, with the fixtures [`open_folder.rs`](#open_folderrs) uses: ids equal no index, window lists come out of stacking order, and no directory exists, so `project_directory` answers from the open directories alone.

#### `other_pane_seam.rs`

**Covers**

- `other_pane_seam::target`'s rules over hand-built `PaneTree`s, in a table that mirrors [`OtherPaneSeamTests.swift`](../../wolf-macos/Tests/WolfKeyMapTests/README.md#otherpaneseamtestsswift) case for case, with the same trees, ids and bounds.

**Gotchas**

- Pure: no display, no Neovim and no compositor.
- A case added on either side belongs on both, or the two ports can drift silently.

#### `packaged_layout.rs`

**Covers**

- The tree `nix build .#wolf-gnome` installs, read out of the store path `WOLF_PACKAGE_OUT` names. The captive `init.lua` is byte-identical to the checkout's and sits at `<out>/bin` joined with the resolver's own `INSTALLED_INIT_LUA_RELATIVE`; both vendored Lua trees are non-empty beside it, carrying what the Bazel filegroups keep and none of what they exclude.
- `bin/wolf` refusing `--bogus` with its usage line; the desktop entry's exact `Exec`, `$out/bin/wolf %f`, and `MimeType`; each icon decoding to the pixel size its hicolor directory names; the wrapper naming every variable it must supply and never `WOLF_INIT_LUA`; both editor font families beside an additive `fonts.conf`; the Symbols Nerd Font Mono icon face under the `share/fonts` that `fonts.conf` names, with both of its upstream notices under `share/licenses/`; the compiled `xterm-ghostty` entry at `share/wolf/terminfo/x/xterm-ghostty`, in ncurses' compiled format, naming `xterm-ghostty` first and carrying the extended `setrgbf` that only `tic -x` keeps.

**Gotchas**

- Every test is `#[ignore]`d, its precondition panics rather than skipping, and one command satisfies it: `./tools/wolf-package-smoke.sh`. Every falsifier here is a `flake.nix` edit.
- An empty sibling tree is worse than an absent one, and is asserted apart from a missing file. `init.lua` gates each tree on an `fs_stat` of the directory, which an empty one satisfies, so `:colorscheme lighthouse` errors, the file aborts mid-way and the application dies naming a function nobody wrote.
- The icons are decoded rather than opened, because the defect worth catching — an export installed into a directory naming a different size — passes every existence check.

#### `paint_golden.rs`

**Covers**

- `paint` onto an offscreen ARGB32 `ImageSurface` from a hand-built 4x2 `Snapshot`: exact equality on known background fills, background-only interiors for a wide-trailing empty cell, and ink in a glyph cell.
- The line-height argument to `CellMetrics::for_font` at the shipped 180 % default: a larger percentage gives a taller cell, the advance width does not move with it, and the baseline centres the glyph box. An unresolvable family still measures, matches a resolvable one's cell height, and stays first, unchanged, ahead of the symbols face.
- The three decorations: undercurl draws ink below the glyph box, in `special` when set and in the foreground when not; undercurl and a plain underline draw different marks; strikethrough adds foreground ink and no `special` pixel.
- The pinned stack resolving "Symbols Nerd Font Mono", which shapes U+F015.
- The two values `render::GlyphPainter` keeps across the cells of one paint: an undecorated cell painted after every decorated one carries no leaked underline, and bold and italic each restore the plain face for the cell after them. The trailing cell is required: the control column precedes every decorated one, so it cannot see a forward leak.

**Gotchas**

- Not a whole-image hash, which is too brittle across freetype/fontconfig versions. Pixel reads assume little-endian ARGB32, in-memory byte order `B, G, R, A`.
- The PNG golden is a dev aid nothing asserts against; regenerate it with `cargo test -p wolf-gnome --test paint_golden -- --ignored regenerate_png_golden`, which needs the `png` feature on the `cairo` dependency.

#### `pane_views.rs`

**Covers**

- The per-pane extras' pure seams: `pane_layout::encode_with_views` writing each leaf's captured view and ignoring one of the other kind, `encode` equal to it with no views, `saved_views` seeding a restored tab's leaves, and `pane_ref` naming a leaf or the drawer.
- `preview_ui`'s scroll report: `reported_scroll` clamping to `[0, 1]` and refusing a non-finite value, `SCROLL_REPORT_JS` posting to `SCROLL_REPORT_HANDLER`, and `scroll_to_fraction_js` formatting only a finite fraction in range.

**Gotchas**

- Pure: no display, no Neovim and no WebKit. Whether the pane view captures at the right moments and the page posts and applies a scroll is [`smoke_headless/extras.rs`](#smoke_headlessextrasrs)'s question.

#### `pane_zoom.rs`

**Covers**

- `pane_zoom`'s rules, K1 to K5 and K7 to K14, with the ids of [`PaneZoomTests.swift`](../../wolf-macos/Tests/WolfKeyMapTests/README.md#panezoomtestsswift). K6 is absent, since it tests the frozen dividers and no divider exists while a view here is zoomed, and K10 tests `shown_alone` over bound ids where the Swift tests `visibleRects`. K11 to K13 compose `focus_origin`, `drawer::focus_target` and `zoomed_after` as `apply_focus` does, over a plus-shaped tree above the drawer.
- The saved zoom through `pane_layout`: G6a pins `leaf_index`, the depth-first index a saved tab's `zoomed` names, and the drawer's `None`. P3 is this file's own, the `leaf_index`, `encode` and `decode` round trip a relaunch makes, since this front-end has no FFI push-builder for the Swift P3 to cover.
- `restore_targets` (E10): a saved zoom and focus mapped onto the rebuilt leaves, the drawer only while it restores open, and a zoom winning the focus.
- `ZoomEvent::OpenFile` (Z1) unzooming whichever pane is zoomed, the drawer included.

**Gotchas**

- Pure: no display, no Neovim and no compositor. What the pane area shows for an answer is [`smoke_headless/zoom_pane.rs`](#smoke_headlesszoom_paners)'s question.
- A case for a rule both front-ends apply belongs on both, or the two ports can drift silently.

#### `preview_root_style.rs`

**Covers**

- `preview_width`'s rules. A capped `root_style`'s content box lands on `editor_inset::grid_rect`'s capped rect in a pane wider and in one narrower than the cap, its width exactly and its left edge within the floor `grid_rect` takes; a full-width cap pads without capping.
- `styled_document` over a real `render_html(…, WidthMode::Full)` puts the style on the `<html>` tag alone and leaves the document byte-identical from `<head>` on, or returns a document it cannot style unchanged; `apply_root_style_js` decodes to exactly the style.

**Gotchas**

- Pure: no display, no Neovim and no compositor. The content box comes from `content_box`, a model of CSS block layout for a `border-box` root; whether WebKitGTK lays the page out so is [`smoke_headless/preview_margins.rs`](#smoke_headlesspreview_marginsrs)'s question.

#### `release_profile.rs`

**Covers**

- The workspace `Cargo.toml`'s two `[profile.release]` promises: that `panic` is `unwind`, and that debug info is emitted and not stripped. The reader is a line scanner, not a parser: the manifest must keep one key per line, with no inline comment after a value.
- The artifact those promises are meant to produce: `CARGO_BIN_EXE_wolf-gnome` carries a non-empty `.debug_line`, and this workspace's own source paths appear in the binary's debug strings — found through a section-header walk, because both needles also sit in `.rodata` of a binary carrying no debug info of ours.

**Decisions**

- `std` only: a `toml` or `object` dev-dependency would add a line to `Cargo.lock`, a `crate_universe` input in `MODULE.bazel`, and the `CARGO_BAZEL_REPIN` that forces cannot be run from a Linux host.

**Gotchas**

- The artifact half discriminates the release profile only under `--release`: a plain `cargo test` hands it the dev binary, whose debug info is on by default. `cargo test --release -p wolf-gnome --test release_profile -- --nocapture` prints the path it read.
- The manifest half is a tripwire on a promise, the same shape as `the_shared_smoke_config_pins_the_appearance_every_ocr_smoke_assumes`: its falsifiers are `Cargo.toml` edits, not `src/` ones.
- `opt-level`, `lto` and `codegen-units` are deliberately unpinned, so a measured retune stays a manifest change rather than a test edit.
- It lives in this crate rather than `wolf-core` because this crate has no `BUILD.bazel` to wire it into, and the workspace manifest it reads is not a Bazel runfile.

#### `restore_caps.rs`

**Covers**

- `state_seam::restore_plans`' two caps: a session restores at most `MAX_RESTORE_NEOVIMS` tabs across every window, frontmost first, each window at most `MAX_RESTORE_TABS`; a window that only partly fits keeps its first tabs, and a session exactly at the budget restores whole.

**Gotchas**

- Pure: no display, no Neovim and no filesystem, since `is_dir` answers `true`. Each test pins both caps as premises first, and every expected count is written out by hand.
- Every window sits on a directory of its own and every tab carries a root, so nothing but the caps can shorten a plan.

#### `roundtrip_nvim.rs`

**Covers**

- The full input seam against a real Neovim: `gdk_key` to `InputEvent::Key` to `WolfCore` to snapshot. It types `Hello` in insert mode, feeding the leading `H` as the already-uppercased keyval plus the `SHIFT` modifier mask exactly as GDK reports it, and asserts row 0 contains the word — the shift-glyph contract end to end.

**Gotchas**

- Requires `WOLF_NVIM_BIN`; `common::require_nvim_bin` panics rather than skipping. No display needed.
- Leaving insert mode emits several follow-up flushes, so the test drains and sleeps a settle window before reading the snapshot. A new case that reads a snapshot needs the same treatment.

#### `sidebar_search.rs`

**Covers**

- The pure seams behind the sidebar's search panel, with no display, Neovim or ripgrep: `search_panel`'s show guard, initial toggles and the options they give; its rows' projection order, exact location, collapse, line cap, windowing and bold shift past an ellipsis; `char_budget`; the selection kept by identity across a rebuild; the Enter rule, with toggles changed under an unchanged query; row activation; the liveness-first open route; and the status line's arms, the stale-toggles one and rg's error exit among them. Also `sidebar::panel_visibility`.

**Gotchas**

- Every input is a hand-built `SearchMatch` or a literal, and every expected value is written out by hand, never computed by the function under test.
- What the widgets do with these answers is the search smokes' in [`smoke_headless/sidebar.rs`](#smoke_headlesssidebarrs) and [`smoke_headless/search_toggles.rs`](#smoke_headlesssearch_togglesrs).

#### `sidebar_toggles.rs`

**Covers**

- The pure half of the header bar's toggle group: `active_toggle` for each shown flag and page, `toggle_click` for every combination of shown, page and clicked, and `toggle_at` either side of the group's middle, left to right and mirrored right to left.

**Gotchas**

- Pure: no display, no Neovim and no compositor. Every expected value is written out by hand.
- Whether the window wires the answers up is [`smoke_headless/sidebar_toggles.rs`](#smoke_headlesssidebar_togglesrs)'s question.

#### `smoke_headless/main.rs`

**Covers**

- The crate root and shared harness of the `smoke_headless` binary, whose smokes launch the built application under `cage` and assert on OCR, raw pixels, Neovim readbacks, files on disk or the exit status. Each feature's smokes are a module beside it. Its `RESIZE_OUTPUT_FRAGMENT` is how a smoke resizes the window, under [Smoke conventions](#smoke-conventions).
- The harness guards: `cage` spawns only through `cage_command`, each cage is isolated from the host session, the slots bound concurrency, every launch waits for readiness, the appearance pin holds, and the column cap cannot bind at the OCR geometry.

**Decisions**

- `CAGE_SLOTS` is per process, so a cage smoke belongs in this binary, where the slot bound counts it.
- `pane_fields` holds the smokes' only `Pane` destructuring, so a new pane kind or field touches that function alone.

**Gotchas**

- Run with `cargo test -p wolf-gnome --test smoke_headless` inside the devShell. `--exact` takes `<module>::<name>`, as in `editor::window_renders_typed_text_under_cage`; the guards here keep bare names.
- Every `*_under_cage` test is a cage smoke and every other test a compositor-free guard; `passed` counts both.
- The source-scanning guards read the crate's test sources at run time: a `cage` spawned under `tests/` outside `cage_command` fails one, and a declared module left unread fails both.
- A failing smoke keeps its runtime directory, with its captures and `cage.log`, and prints its path and the log's tail.
- Spell checking is off in every smoke, so OCR reads glyphs not undercurls; see `seed_smoke_spell_override`.

#### `smoke_headless/close_pane.rs`

The Close Pane smoke, one launch restoring a planted window of two tabs of one pane each, each on a file of the project, with the selected second tab then modified. Ctrl+Shift+W there must keep both Neovims and leave E37 in its `:messages`. After `:w` it must close the tab, leaving the first, and in the last tab end the app. Every observable is a Neovim count, a readback a Neovim writes about itself or the app's exit, never OCR.

It clicks nothing, because keys reach the selected tab after a restore, a premise exit 191 checks.

#### `smoke_headless/close_tab.rs`

The Close Tab smokes, each one launch restoring a planted window whose tabs are on files of the project. Every observable is a Neovim count, a readback a Neovim writes about itself or the app's exit, never OCR.

`close_tab_asks_the_tabs_neovim_under_cage` restores two tabs, with the second modified. A click on the background second tab's close button must select it, keep both Neovims and leave E37 in its `:messages`, and Close Tab from the palette must refuse the same way. After `:w` it must close the tab, leaving the first, and on the last tab end the app.

`close_tab_with_no_answer_on_the_last_tab_closes_the_window_under_cage` restores one modified tab whose Neovim's `_G.wolf_close_tab` is made to answer no boolean, so the ask gets `NoAnswer`. Close Tab from the palette must then end the app, because the last page's close confirms its tab and the window's check does not ask it again.

The close button's x is `CT_CLOSE_BUTTON_INSET_PX` left of the output's right edge, derived from libadwaita's stylesheet rather than measured, and a background tab shows its button only while hovered. Exit 184 or 185 can mean the click missed the close button, and then the inset is what to fix, not the code.

#### `smoke_headless/close_window.rs`

The Close Window and Quit smokes, each one launch restoring a planted window of two tabs on files of the project, with the background second tab modified. Every observable is a Neovim count, a readback a Neovim writes about itself or the app's exit, never OCR.

`close_window_asks_every_tab_under_cage`: Close Window from the palette in the clean tab must keep both Neovims, select the modified tab and leave E37 in its `:messages`. After `:w`, `:qa` there must close only its own tab. Close Window on two clean tabs, the one left and a new one, must end the app with neither Neovim left.

`quit_asks_every_window_under_cage` opens window 2 from the palette, and a readback naming its buffer, `Untitled.md`, which is none of window 1's files, proves the keys reach it. Ctrl+Shift+Q there must quit nothing. `:WolfCloseWindow` in clean window 2 must close it, which shows that Close Window asks only its own tabs. The keys then pass to window 1, whose readback and `:messages` show that the refused quit selected the modified tab and echoed E37, and after `:w` Ctrl+Shift+Q ends the app.

The palette queries are asserted to rank Close Window and New Window first. Measured by the green runs: a page selected inside `close-request` is the tab keys reach, and once cage's newest toplevel closes, the one below takes the keys, which exit 216 checks. The quit's raise of window 1 is unobservable, because cage declines `present()`.

#### `smoke_headless/compose.rs`

Input-method smokes: composed text reaching Neovim, and a control chord typed while a compose sequence is pending.

#### `smoke_headless/config.rs`

Configuration smokes: `wolf-config.yaml`'s font size and appearance reaching the grid and Neovim, and the column cap with the full-width toggle.

#### `smoke_headless/drawer_divider.rs`

The drawer divider's smokes. One launch drags the handle between the editor tree and the drawer through [Pointer injection](#pointer-injection), reading back Neovim's window heights and the drawer shell's `stty size`. The drag must grow the drawer's window and its session, survive the drawer closing and reopening, give the editor back its launch height while the drawer is closed, stop at the editor's floor when released above the pane area, and stay out of a second tab New Tab opens, whose drawer opens at a new share.

A second test, `a_dragged_share_survives_a_window_shrink_and_grow_under_cage`, drags the share to just above the editor's floor, shrinks the window until that share's position clamps to the floor, and grows it back. The editor's and drawer's heights must return to the drag's, so the shrink's clamp was not taken for a drag. Its premises derive through `terminal.rs`'s `drawer_paned_at`.

A third, `each_tab_keeps_its_drawer_share_across_a_relaunch_under_cage`, drags the drawer in one of two tabs. The other tab's drawer must open at a third, each tab must show its own share on a switch, both must be saved open with their own shares, and a relaunch must restore each.

The handle's y comes from `terminal.rs`'s `drawer_strip_edges`, and no assertion reads a pixel.

#### `smoke_headless/editor.rs`

Editor smokes: typed text, the file and buffer pickers, the empty state, a new untitled buffer, the command palette, navigation history, completion, messages and the confirm prompt, the clipboard, and saving. The multi-line message smoke also reads the toast's box from the capture's pixels: as wide as its widest line's cells, a wide character paying two, plus the padding on both sides, and no wider.

#### `smoke_headless/extras.rs`

The per-tab and per-pane extras' smokes, each over the saved state frozen at its capture moments and none reading OCR:

- `pane_cursors_are_captured_and_restored_under_cage`: edit cursors and top lines captured on a focus change, a tab switch and quit, and not between them, then restored with each tab's focused pane.
- `preview_scroll_survives_a_close_and_relaunch_under_cage`: a wheeled preview's scroll captured as its window closes and applied after the relaunch's load. The relaunch wheels the restored page further, so its quit must save the restored scroll plus that wheel, and a restored scroll that is never applied does not pass.
- `each_tab_keeps_its_sidebar_page_under_cage`: each tab of one window saving its own sidebar page.
- `a_zoomed_focused_drawer_restores_under_cage`: a drawer saved open, zoomed and focused reopening that way with a shell in the project, at its saved share once unzoomed.
- `a_restored_top_line_survives_its_grid_resize_under_cage`: a top line restored with the cursor below the top row surviving the grid resize that lays out its pane. It launches at its own font size, `RV_FONT_PT`, so the pane has room for the saved view.

#### `smoke_headless/fixture_guards.rs`

The guards behind the fixture-naming rule in [Smoke conventions](#smoke-conventions). A smoke that OCRs a capture registers each term it asserts on in `ASSERTED_OCR_TERMS`, which the guards check every fixture against; a smoke that OCRs nothing registers none.

#### `smoke_headless/frame_stats.rs`

The frame-time instrument's smoke: typing bursts with `WOLF_FRAME_STATS` armed, read from the report file the application appends to.

#### `smoke_headless/header_bar.rs`

The header bar's contents smoke: one keyboard-only launch, so no pointer hover reaches the capture, and one capture before any key. The header band's columns are read as ink against the bar's own background, the dominant colour of the span the titlebar smokes read as one flat fill. The run of ink holding the Search toggle's centre must start where the group follows Back/Forward, within half an image button, and nothing may lie right of it, so the start holds Back/Forward and the group alone, as the macOS toolbar does. Then `Ctrl+Shift+O`, a query and Enter must open the fixture, read back as Neovim's buffer name.

#### `smoke_headless/lighthouse.rs`

Not a smoke: the oracle that reads expected colours from the pinned `colors/lighthouse.lua` in the tree `WOLF_TEST_LIGHTHOUSE_DIR` names, a copy of wolf-ffi's `common::Lighthouse` that parses the same shapes ([`//bazel/lighthouse`](../../../bazel/lighthouse/README.md#the-pin)). It panics when the variable is unset, since the devShell always exports it, and is read in the test process only: the packaged smokes scrub the variable from the cage. Its `stage_init_lua` stages the checkout's `init.lua` beside a copy of the pinned colorscheme, for a smoke that runs the real Lighthouse.

#### `smoke_headless/file_open.rs`

The `wolf` command's smokes, each over a planted state. They run the `wolf` beside the app binary, which Cargo names in no variable for another package, and fail when it is absent ([Building and testing](../../../build.md#building-and-testing)). `wolf D` from the empty state fills the empty current tab with `D`, adding no Neovim. `wolf FILE` lands by each [§5.2](../../../specs/cli-and-file-open.md#52-choosing-the-pane) rule, one tab per rule: the active editor pane, on a missing file left off disk; the larger leftmost editor pane; a preview made an editor; the pane already on the file focused; and a zoomed tab unzoomed first.

A terminal pane's shell sees its tab's `WOLF_TAB_ID`, distinct per tab, and its `wolf` lands in that tab through the whole-tab split; a stale ID goes to the file's project. With no app running, `wolf FILE` launches the app beside it, which restores both planted windows, adds none for the command's directory, and places the file after the restore.

`wolf --wait` serves `git commit` typed into a tab's terminal, committing the typed message on `:wq`, and one from outside Wolf, which `:cq` aborts with no commit. `:wq` on a tab's last pane, which closes that tab alone, still exits the waiting `wolf` `0`.

Every request lands in the window that holds the keyboard, and no smoke asserts a raise, since `cage` raises no background window. Each request is followed by a polled Neovim readback, because `wolf` exits `0` once the request is taken, not handled.

#### `smoke_headless/new_tab.rs`

The New Tab smokes, each launching on a folder tab and opening a folder `D` through its Open Recent row, then reading the save the quit leaves. New Tab must open an empty tab beside it: a live Neovim at `HOME`, the empty-state page, no sidebar, and `wolf` in its bar entry and in the header once it is the window's only tab. A `D` with no project state opened into that tab must show the sidebar exactly when the origin showed it at New Tab, after the origin has closed too. A `D` whose project state hides the sidebar must hide it over a shown origin, and New Window's empty tab must show the sidebar whatever its origin showed.

`wolf` is in no fixture name, but `TEMP_CWD_PREFIX` spells it, so the term is not in `ASSERTED_OCR_TERMS`; this file's own guards check what its smokes put on screen. No Ex command is typed before a capture in the tab it shows, since the cmdline strip would draw the runtime dir's path.

#### `smoke_headless/open_folder.rs`

Open Folder and New Window smokes over a state each plants in its own store, by submitting a tree holding `D` and then one without it: an Open Recent row for `D` open in a background tab of the back window selects that tab and opens nothing; from an empty tab, fills it with `D`'s two-pane split; from a folder tab, opens a new window restored to that split and its stored size, leaving the launch tab alone.

The [§7](../../../specs/application-state.md#7-new-windows-and-tabs) smokes plant a launch window at a size neither the default nor the output's: New Window, and an Open Recent row for a folder with no project state, each open a window at that size. Under `cage` the launch window is maximized, so New Window also proves a maximized window lends its normal size. They read the save the quit leaves and a live `getcwd()` / `winnr('$')` readback, never pixels: a tab contributes its seed until its first gather succeeds, so a save can echo a restore that never applied.

#### `smoke_headless/packaged.rs`

The `#[ignore]`d packaged smokes, which drive the store binary `nix build .#wolf-gnome` installs, scrubbing every devShell export the artifact supplies itself: a clean OCR launch, an A/B against the checkout binary over the two vendored Lua trees, a preview pane, and a terminal pane whose shell gets `TERM=xterm-ghostty` and the package's `TERMINFO`, with `infocmp -x xterm-ghostty` succeeding there. The A/B reads Lighthouse's colours through [`lighthouse.rs`](#smoke_headlesslighthousers). They run only through `./tools/wolf-package-smoke.sh`, which selects them with the filter `packaged::`.

#### `smoke_headless/panes.rs`

Pane and layout smokes: splits, focus moves, closing panes, `:q` and `:qa`, the per-pane status bar, and native tabs, where each tab must run a Neovim process of its own, New Tab's must not start in the selected tab's directory, and closing one must shut that Neovim down. A file saved in one tab must reload in another when it is selected, through the focus trigger's `:checktime` alone.

Of two tabs, `:q` on one's last pane and `:qa` in a split one must close that tab alone, leaving the window and the other tab's Neovim. In a one-tab window `:qa` closes the window, through `tabs_ui::close_outcome`'s last-tab arm.

#### `smoke_headless/pointer.rs`

Pointer smokes, driven through [Pointer injection](#pointer-injection) and read back from Neovim: click, drag, wheel scroll, click-then-close keyboard focus, and a divider drag across a relayout.

#### `smoke_headless/preview.rs`

Markdown preview smokes: opening a preview and returning to the source, a link click that also makes the preview it repoints active, a click in a preview's page, or in its top padding gutter, making it active, read from Neovim's current window and from the strips' pixels, an unsaved buffer, the configured appearance, a view created after launch, Ctrl+Shift+P and Ctrl+Shift+W still firing after a click in the preview, and Preview and Edit in Another Pane reusing the other pane, where a press from a lone pane splits. Its guard keeps the smokes' burst delay below the coalescer's trailing delay.

#### `smoke_headless/preview_margins.rs`

The preview's content-width smoke, read from raw pixels and the frozen window state, with no OCR. One launch at `FULLWIDTH_SMOKE_FONT_PT`, where the cap binds, previews a code block above several viewports of one-line paragraphs in a lone pane. The page's background must span the pane below the strip with the code block on the capped grid rect, `:WolfFullWidth` must widen the block to the pane less its padding, and a second toggle must bring it back. A wheel over the left side margin must then scroll the page, read from the saved preview scroll; every failed assertion is reported by its ID.

#### `smoke_headless/preview_render.rs`

Preview rendering smokes read from raw pixels: fenced-code highlighting, the code block's background in each configured appearance, the baseline grid under inline code and the baseline below h3, h4 and h6 headings, and the file picker drawn over a live preview, with guards on the thresholds those pixel readings use.

#### `smoke_headless/recovery.rs`

Recovery smokes: a crashed Neovim restarted with its file, a stalled one raising the stall overlay, the crash overlay's Restart button, and a background tab's Neovim killed while another tab is selected, which must restart that tab alone with its file while the other tab's Neovim lives on and the window keeps both tabs.

#### `smoke_headless/restore.rs`

Session-restore smokes, each two launches in one cage, the second reading what the first saved: the pane layout, a lone file, tabs, each restored with a Neovim of its own that holds its own file, dragged divider weights, and two windows, saved in stacking order with their tabs, active tabs and a zoom, and restored with the frontmost in front on its saved active tab.

Two more launch once: over a stale `state.json` and `state.json.tmp`, which must be gone once the app is ready, with nothing of them restored; and over a planted state of three windows, which the save at the quit must list in the planted order.

#### `smoke_headless/scrub_lists.rs`

The packaged smokes' scrub lists, the devShell exports removed from a packaged launch. [`flake_exports.rs`](#flake_exportsrs) compiles this file through `#[path]`, so its guard reads the lists the packaged smokes use; which list takes which export is in the [`flake.nix` notes](../../../notes/flake.nix.md#shellhook-exports).

#### `smoke_headless/search_toggles.rs`

The search panel's toggle smokes, one each for case, regex, an invalid regex such as `foo(bar`, which must show rg's error in the status line and then be found literally with regex off, and all files, where the panel opens Markdown-only and the Enter after `All` goes on finds the `.txt` too. What a search found and what the status line says are OCR of the sidebar band. Where the toggles sit, left-aligned below the field and above the results, is the changed rows and columns between captures of one run, read from the band's raw pixels. Every capture is taken after Escape has handed focus back to the editor, so neither the field's caret nor its focus ring is in the band.

The click targets are estimated from libadwaita 1.9.3's stylesheet and a measured label width, not from a probe, which is safe only because a click that misses a toggle changes no option and fails.

`a_restored_search_sidebar_runs_its_query_under_cage` launches over a tab saved on its Search page: the band must show what its query and toggles find, the save must hold them, and a later edit of the field must reach the save at quit.

#### `smoke_headless/sidebar.rs`

Sidebar and search smokes: the file tree, the project-search panel and its chord, the sidebar toggle, a typed `:cd` re-rooting the sidebar, an Open Recent row from a folder tab opening a fresh window on a folder with no project state, and the window title.

#### `smoke_headless/sidebar_toggles.rs`

The header bar's toggle-group smokes. One clicks the toggles through [Pointer injection](#pointer-injection) to switch, hide and show the sidebar, a Search click putting typed keys in the field; the other moves the group with Ctrl+Shift+B, Ctrl+Shift+F and `:WolfSidebarShowProject`. The sidebar's state is OCR of the sidebar band, and the group's is the raw pixels of the header bar's start, compared between captures of one run, so neither smoke navigates after its first capture.

The click targets are derived from libadwaita 1.9.3's header metrics, not from a probe, which is safe only because a click that misses the group moves no sidebar and fails.

#### `smoke_headless/split_commands.rs`

The split-command smokes: from an editor, a terminal and a preview pane, the palette's Split Vertical and then, in the new pane, Split Horizontal must save a root of `vertical[pane, horizontal[pane, pane]]` whose every pane is of the first pane's kind. A saved pane carries no session id, so a new session is wolf-core's `split_commands.rs` question, and so is the drawer, whose split the front-end drops.

#### `smoke_headless/symbols.rs`

The symbols-face smoke: one launch at the smoke config shows `M` and the icons U+F07B, U+E0A0 and U+F401 in an editor buffer, set through `nr2char`, and in a `:terminal` whose shell prints them. In each `grim` capture, `M` must match DejaVu Sans Mono alone, every icon the symbols face alone and not the hex box, and U+F401 must differ from DejaVu Sans Mono alone. The references are painted in the test process through `wolf_gnome::paint`, on the surface kind each pane paints on, and the preconditions hold the grid origin and the cell to the values the pointer smokes use.

#### `smoke_headless/tab_bar.rs`

The tab bar smoke: one launch restores a planted window of two tabs on sibling directories under the smoke's `HOME`, so neither is the other's prefix. Both tabs hide the sidebar, whose root row would also draw the selected tab's directory, so only the bar and the header draw one. The header draws the selected tab's, so each OCR read is of a background tab's bar entry: tab 0's after its own `:cd`, and tab 1's after a `:cd` it runs in the background, which must replace its old title.

#### `smoke_headless/terminal.rs`

Terminal smokes: the terminal drawer, terminals across a Neovim restart, terminal panes beside Neovim panes, typed keys reaching a pane's shell, Ctrl+`` ` `` from an editor focusing the open drawer, which a second Ctrl+`` ` `` then hides, Ctrl+`` ` `` hiding a showing message toast as it opens the drawer, and Ctrl+Shift+W closing the drawer with its session kept and a terminal split with its session hung up. Two more cover the tab's last pane beside the drawer: `:bd` in the only editor pane keeping it, current, on the earlier file, and able to take focus again after the drawer has had it; and a file the picker or a search result opens while the drawer is focused landing in an editor pane, the search result at its line, with the drawer kept.

#### `smoke_headless/terminal_colours.rs`

The terminal-colours smoke: two launches of the checkout binary in one cage, over a staged `init.lua` beside a copy of the vendored Lighthouse, with `appearance` set to `dark` then `light`. In each, a pane's 16 ANSI background cells must paint Lighthouse's colours for that background, read through [`lighthouse.rs`](#smoke_headlesslighthousers), and every run prints a table of expected and measured colours. The cells' pixel rects are derived from the pane geometry, with no pixel literal.

#### `smoke_headless/terminal_terminfo.rs`

The terminfo smoke: the checkout binary, given `WOLF_TERMINFO_DIR` from the devShell's `WOLF_TEST_TERMINFO_DIR` and decoy `TERM` and `TERMINFO`, opens a pane by typed `:terminal` whose shell reports `TERM=xterm-ghostty` and that `TERMINFO`. It proves that `window.rs` hands core the directory; core's choice is wolf-core's `terminal_terminfo.rs`. Its shell script, scenario and readback are shared with the packaged terminfo smoke.

#### `smoke_headless/titlebar.rs`

The titlebar smokes, one per Lighthouse background: the checkout binary over a staged Lighthouse, with `appearance` forced to that background while `ADW_DEBUG_COLOR_SCHEME` makes the desktop prefer the other. One launch opens a second tab and takes four captures: focused, after `:set background` to the other, after a runtime `WolfTitlebar`, and in backdrop with GTK's in-process save dialog up, so the smoke exports `GDK_DEBUG=no-portals`. The header bar and the tab bar must match the grid's `Normal`, then `WolfTitlebar`, and the title and the tabs' text must be drawn on the forced scheme's side. The bands derive from the geometry constants, and every run prints a table of expected and measured colours.

The dialog is the route to backdrop: `cage` activates every toplevel it maps and deactivates one only when another takes focus, so no launch starts inactive. A toplevel with a parent is centred at its own size, so the main window and both bars stay visible beside it. A precondition proves the dimming: the title's contrast over `WolfTitlebar` must reach a floor while focused, and be at most a share of it in backdrop.

The empty-view smoke opens an empty tab over Lighthouse under `appearance: dark` and samples the view's two side bands, clear of the status page and derived from the geometry constants. The view must match the grid's `Normal`, then the other background after a typed `:set background`, and keep it after a runtime `WolfTitlebar` the header bar takes. An OCR precondition proves the view is up.

#### `smoke_headless/titlebar_tabs.rs`

The `gtk-titlebar-style` smokes. The first launches on the shared seeded config, so it runs the default `tabs`. With one tab, the header band must read the window title and the probe click the one-tab pane origin. A second tab, `:cd`ed into a sibling directory, must put both tabs' titles in the header band, take no editor row, leave the header's shade row where it was by `TABS_HEADER_GROWTH_PX`, and paint the tabs in the grid's background; a probe click and a delta click in it must land.

A middle-click on the second tab's header entry closes it, and the title, the header's height and typed input must all return. The second smoke pins `native`: with two tabs, the header must read the window title alone, the row below it both tabs' titles, and the editor lose rows. `titlebar_pins_parse_to_their_styles` is a compositor-free guard that the shared config and each pinned one parse to their style with the smoke font kept.

The header band is cropped down to each capture's own shade row, measured rather than assumed, so a header that grew moves it.

#### `smoke_headless/tree_reveal.rs`

The sidebar launch-reveal smoke. One launch restores a tab with the tree sidebar beside an editor pane, focused, and a preview pane, both on a file below the fold, over a generated folder of about three hundred mixed-case root files, four subdirectories and the usual dot entries. Capture A must read the six rows directly above the current file and its own row; the scenario then creates a file, so the watcher's rescan rebuilds the tree, and capture B must read them again and the new row. The scenario sends no key and no pointer event, so nothing but the reveal scrolls the list.

The asserted rows are the subject, so their generated names carry their terms. `reveal_fixture_puts_its_rows_directly_above_the_current_file` guards that premise without a compositor: the rows' order under `wolf_core::tree::children_of`, the row counts above and below, and each term naming one row.

#### `smoke_headless/zoom_pane.rs`

The Zoom Pane smokes. One launch builds `vertical[A, horizontal[B, C]]` over the drawer, drags the tree divider and the drawer handle, then zooms B from the palette. B must fill the pane area while the others keep their size, and each unzoom must restore every pane, both drags included. Zoom Pane again, `<C-w>h`, Neovim's own `<C-w>w`, Toggle Terminal focusing the open, hidden drawer or opening a closed one, a split and Preview in Another Pane must each unzoom, and a focus move must land on the pane it names in the unzoomed layout.

A second test, `a_zoomed_pane_survives_relaunch_under_cage`, quits with a pane zoomed and relaunches. Run 1's save must zoom that pane, asserted before run 2 so a lost save is told apart from an ignored restore, and run 2 must start zoomed on it.

A third, `an_unzoom_after_a_window_resize_fits_the_new_size_under_cage`, zooms B over the same dragged layout and resizes the window. B must fit the new size while the hidden panes keep theirs, the unzoom must match a plain resize to that size, and both drags must survive the return to the launch size. It assumes the sidebar returns to its launch width with the window.

Every observable is a Neovim readback of `winwidth` and `winheight` per recorded window id, the grid size the front-end drives, or the save; no assertion reads a pixel. The tree handle's x is modelled from `geometry::divider_position`, as `drawer_strip_edges` models the drawer handle's y, rather than found by bisection; a missed press fails the drag precondition.

#### `stacking.rs`

**Covers**

- `StackingOrder`: a focused window moving to the front, a minimized one behind every window that is not, focus unmarking it, the seed keeping the saved order and minimized marks, and unranked windows following the ranked ones in registry order. `TabRecency`: the seed's active tab first, and a selected tab becoming the most recent.

**Gotchas**

- Pure: no display, no Neovim and no compositor. Whether the `is-active` handler feeds `focus` is [`smoke_headless/restore.rs`](#smoke_headlessrestorers)'s two-window smoke's question.

#### `state_isolation.rs`

**Covers**

- That [`common::config_in`](#commonmodrs) keeps a real-nvim test off the user's Wolf state dir. The visible test re-executes its own binary twice, each child under a scratch `HOME` holding a planted `.local/state/wolf/runtime_user/init_user.lua` that sets `ignorecase`, with `WOLF_STATE_DIR`, `XDG_STATE_HOME` and `XDG_DATA_HOME` removed.
- `isolated_payload` asserts that the harness moved `state_dir()` off the plant, that `ignorecase` stays off, and that nothing was written under the plant. `control_payload` aims `WOLF_STATE_DIR` at a second plant and asserts that the harness kept it and `ignorecase` is on.

**Decisions**

- A Linux-only port of wolf-ffi's guard, whose [entry](../../wolf-ffi/tests/README.md#state_isolationrs) gives the reasons for the re-exec and the control payload. The scratch homes come from `common::unique_temp_dir` and are deleted on drop, not from `tempfile`, for [`release_profile.rs`](#release_profilers)'s lock-file reason.

**Gotchas**

- Requires `WOLF_NVIM_BIN`; unlike wolf-ffi's guard, the parent panics through `common::require_nvim_bin` rather than skipping.
- Both payloads show as ignored and return at once without `WOLF_TEST_PLANTED_HOME`, so `--include-ignored` never runs one against the real `HOME`.

#### `symbols_fallback.rs`

**Covers**

- `render::family_list`: the configured family, then the symbols face, with no duplicate when a member already names that face in any ASCII case or spacing.
- `CellMetrics::for_font` measuring the configured family alone, at two sizes, and the pointer smokes' literal cell.
- The grid, for iA Writer Mono S, DejaVu Sans Mono and an unresolvable family in each face at two sizes, and a `TerminalCanvas` over a real session: each icon matches the symbols face alone and differs from Pango's hex box, U+F401 differs from the configured family alone, and ASCII matches it. On the grid, an icon's overhang survives the next cell's fill, and a list naming a missing symbols family paints as the configured family alone.
- JetBrains Mono's U+E0A0 and DejaVu Sans Mono's U+2665 keeping the configured face.

**Decisions**

- U+F401 is the discriminator: fontconfig's own sort already reaches the symbols face for U+F07B, but serves U+F401 from DejaVu Sans.
- Each reference is the same content through the same painter with one family, so glyph placement is identical by construction. A match is the one-pixel-tolerant `ink_distance`, a difference the exact `ink_xor`.

**Gotchas**

- The terminal case requires `WOLF_NVIM_BIN` and `/bin/sh`. A failure beginning `premise:` means the pinned faces cannot tell the outcomes apart; it lists every failed premise with the distance table.

#### `tab_bar_slot.rs`

**Covers**

- `tab_bar_slot::tab_bar_placement`'s table, TS1 to TS3: zero or one page is `BelowHeader` under either style, two pages or more are `InHeader` under `Tabs`, and `Native` is `BelowHeader` at every count.

**Gotchas**

- Pure: no display, no Neovim and no compositor. Whether the window applies the answer, and applies it again as tabs open and close, is [`smoke_headless/titlebar_tabs.rs`](#smoke_headlesstitlebar_tabsrs)'s question.

#### `tab_close.rs`

**Covers**

- `tabs_ui::close_page_now`'s table: a live tab is asked once and only `NoAnswer` closes it, while a dead, stalled or gone core and a confirmed close close without asking.
- The tables of `tabs_ui::tab_unsaved`, `window_close` and `quit`: a confirmed tab is `Clean` and a core not `Alive` is `Unreachable`, both unasked, and `NoAnswer` is `Unreachable`. The first `Unsaved` is shown, windows in the order given, and nothing after it is pulled; `Unreachable` beside `Clean` closes everything.
- The `win.close-tab` row: `Command::CloseTab`, no accelerator or fold, `MenuOnly`, `action_for_command`'s answer, and the menu item directly before Close Window.

**Gotchas**

- Pure: no display, no Neovim and no compositor. Whether `on_close_page` passes the page's own confirmation, liveness and ask, and selects a background page first, is [`smoke_headless/close_tab.rs`](#smoke_headlessclose_tabrs)'s question. Whether the close handler and `Coordinator::quit` ask and show the tab is [`smoke_headless/close_window.rs`](#smoke_headlessclose_windowrs)'s.

#### `tab_focus.rs`

**Covers**

- The tab focus trigger's rule: `tabs_ui::focused_tab` answers the selected tab while the window is active and none while it is not, and `tabs_ui::focus_edges` sends nothing for an unchanged focus, the loss before the gain on a switch, and one edge on activation or deactivation.

**Gotchas**

- Pure: no display, no Neovim and no compositor. Whether the edges reach the right tab's core is the reload smoke's question, in [`smoke_headless/panes.rs`](#smoke_headlesspanesrs).
- The activation and deactivation rows are this file's alone. Under `cage` a window turns active as it maps and resigns only when another toplevel maps, such as a dialog or a later window, and no smoke checks what either edge does.

#### `tab_titles.rs`

**Covers**

- `tabs_ui::tab_label`'s rules, TL1 to TL6: a folder tab is titled `~/…` under home and by its absolute path elsewhere, with the absolute path as its tooltip, Pango-escaped there and not in the title. An empty tab, or a folder tab whose Neovim has reported no directory, is titled `WINDOW_TITLE_FALLBACK` with no tooltip.

**Gotchas**

- Pure: no display, no Neovim and no compositor. Whether `TabHandle::refresh_label` passes the tab's emptiness is [`smoke_headless/new_tab.rs`](#smoke_headlessnew_tabrs)'s question, and whether each tab follows its own Neovim live is [`smoke_headless/tab_bar.rs`](#smoke_headlesstab_barrs)'s.

#### `terminal_paint.rs`

**Covers**

- `terminal::canvas::TerminalCanvas` painting a real session's frame onto an offscreen ARGB32 surface: truecolour backgrounds exactly and foreground ink within a tolerance; an inverse cell and a drag selection swapped; the block cursor in its fill and a bar after `\033[6 q`; an unfocused cursor painted hollow over a block and a bar, the focused one as a block, and a cursor hidden by `\033[?25l` staying hidden; `update` returning every row first, then the echoed row but not row 0 nor every row, then none, then every row for new metrics; and the rect's padding and clipping.

**Gotchas**

- Requires `WOLF_NVIM_BIN` and `/bin/sh`: each case opens a session in a pty that prints a fixture and `exec`s `cat`, and polls the frame until the fixture shows. No display.
- Each case asserts, as a premise, that its fixture colours differ from the frame's defaults, and samples one pixel inside every cell edge, since cell origins are fractional. A background under a glyph is read in the band above the glyph box, which the 180 % line height leaves empty.

#### `terminal_seams.rs`

**Covers**

- Table-driven cases for the pure seams under [`src/terminal/`](../src/terminal/README.md) (`style`, `damage`, `geometry`, `input`), the `WOLF_SMOKE_TERMINAL` parse, `scrollbar::terminal_viewport`, `clipboard_ui::copy_decision`'s terminal arm, `liveness_ui::terminal_restore`, and `pane_layout::encode`'s terminal pane.

**Gotchas**

- Pure: no display, no Neovim and no session. Every expected value is written out by hand rather than computed by the function under test.

#### `titlebar_background.rs`

**Covers**

- `titlebar_background::titlebar_background`'s precedence: a set `WolfTitlebar` beats `Normal`, `Normal` fills in when it is unset, and `Libadwaita` holds while neither is known. `empty_state_background` takes `Normal` when it is known and `Libadwaita` while it is not.
- `titlebar_css` and `empty_state_css` writing lowercase `#rrggbb` in channel order and giving `""` for `Libadwaita`, the first declaring no `color` and the second `background-color` alone.
- `stylesheet_update` answering `None` for equal inputs and, for each change, the stylesheet of the function it is given, run over both.

**Gotchas**

- Pure: no display, no Neovim and no compositor. Whether the window paints the answer is [`smoke_headless/titlebar.rs`](#smoke_headlesstitlebarrs)'s question.

#### `treesitter_languages.rs`

**Covers**

- That `flake.nix`'s `wolfTreesitterLanguages` and the `srcs` of `parsers_tree` in `bazel/treesitter-parsers/BUILD.bazel` name the same languages. Neither build reads the other's list, so a one-sided edit otherwise builds green on both platforms while one of them highlights a language the other shows as plain text.
- That each list is strictly ascending, which also rules out a duplicate, and both scanners' non-vacuity: each list is found, non-empty, and holds no line its scanner cannot read.

**Gotchas**

- Pure and not `#[ignore]`d, so it runs on every `cargo test`. It lives here for [`release_profile.rs`](#release_profilers)'s reason.
- Each side is a line scanner with a one-entry-per-line contract: a `"<lang>"` line in the flake, a `"@ts_<repo>//:<lang>_parser",` line in Bazel, whose language is the target name, so `@ts_typescript//:tsx_parser` reads as `tsx`. Reformat the file; do not teach the scanner a second shape.
- It names no language and no count, so a language added to both sides in one change keeps it green untouched.
- A membership failure lists the languages found on only one side, in both directions, in one assertion.

#### `vendored_pins.rs`

**Covers**

- That `flake.nix`'s `wolfVendoredPins` block and each asset's `bazel/<asset>/extensions.bzl` name the same revision and the same sha256 for iA Writer Mono S, `orlando`, `lighthouse` and Symbols Nerd Font Mono. A one-sided bump is otherwise silent: each side verifies its own download against its own hash and neither reads the other's, so both platforms build green while shipping different revisions of a vendored asset.
- That both sides state each revision under its row's kind: `commit` and `_COMMIT`, forty lowercase hex characters, or `version` and `_VERSION`, dot-separated digit runs.
- Both scanners' non-vacuity: one Nix entry per row under the row's name, exactly one revision line and one `_SHA256` line per Bazel file, and every sha256 sixty-four lowercase hex characters.

**Gotchas**

- Pure and not `#[ignore]`d — no environment, no subprocess, only `flake.nix` and the extensions read — so it runs on every `cargo test`. It lives here for [`release_profile.rs`](#release_profilers)'s reason: neither file it reads is a Bazel runfile.
- Each side is a line scanner rather than a parser, so each imposes a formatting contract the file it reads carries a reciprocal comment about. Reformat the file; do not teach the scanner a second shape.
- Mismatches are collected and reported together, since bumping two assets at once is the ordinary shape and a loop of assertions reports only the first.
- It fetches nothing, so it cannot say either hash is the hash of the archive at that revision — only that the two sides agree.

#### `common/mod.rs`

**Covers**

- Re-exports, for this crate's integration tests, the helpers the three crates' tests share from [`wolf-test-support`](../../wolf-test-support/README.md), whose [`src/README.md`](../../wolf-test-support/src/README.md) owns their rules: `require_nvim_bin`, `unique_temp_dir`, `config`/`config_in`, `FlushWaiter` and `wake_channel`, `start_core_sized`, the terminal-session helpers, and the grid and timeout constants. It defines nothing of its own.

**Gotchas**

- `config_in` is the state-dir funnel, so build every `CoreConfig` through `config` or `config_in`. [`state_isolation.rs`](#state_isolationrs) is the guard.

#### `../testdata/fonts.conf`

Template fontconfig that exposes only the pinned font directories — DejaVu, JetBrains Mono, iA Writer Mono S and Symbols Nerd Font Mono — and deliberately omits every host font dir, so Pango measurement and Cairo rasterization are reproducible across machines. The two editor families are exposed because the shared `wolf-config.yaml` can name either, and the icon face so a painter can resolve Private Use Area icons, which no editor family carries.

Every generic family the application can ask for is aliased to a pinned face, which is what keeps a generic request deterministic with every pinned family visible: `monospace` to DejaVu Sans Mono for a grid request, and `sans-serif` and `sans` to DejaVu Sans for the pane statusbar strip, which asks for the desktop UI family.

The aliases reach generic requests only, and `gtk-font-name` usually names a concrete family the pinned trees lack. A last-resort rule appends `DejaVu Sans` to every pattern at lowest priority: a resolvable request is untouched, an unresolvable one lands on that pinned face, so the strip's face is the same on every host rather than a function of the developer's desktop setting.

The flake's `shellHook` substitutes its placeholders with Nix store paths and points `FONTCONFIG_FILE` at the generated copy, so this file is never consumed directly. Edit it here; the copy is rebuilt on each `nix develop`.

## Smoke conventions

The cage slots and private runtime directories, the per-cage session bus that activates nothing, the pinned GTK settings, the single-shell-command shape, and the single environment helper are in [wolf-gnome-decisions.md](../../../notes/wolf-gnome-decisions.md). What follows is what a smoke author has to get right.

**A fixture must not be named after the term its smoke asserts on.** The sidebar renders every file in the working directory by name and nvim's `:e` echoes the filename, so a fixture carrying the asserted word satisfies the assertion whether or not the feature did anything — a permanent silent pass, not a flake. Fixture names and asserted terms are therefore deliberately unlike: a heading lives only in a file's content. The exceptions are the smokes whose subject is a tree row, such as the sidebar-population and launch-reveal smokes, where the rendered row is the feature under test; [`fixture_guards.rs`](#smoke_headlessfixture_guardsrs) lists each such term.

That rule has three mechanical guards, because the fixture and the assertion it can disarm sit in different files: one over the sidebar filler names, one over the temp working directory's fixed path components, which the header bar renders as page text, and one over the smoke-owned relaunch fixtures.

The third reaches past names to the fixture bodies an assertion's line, column or rendered text is derived from, and to the crossed pairs behind every exemption, in which neither member may carry the other's term. Each guard's tables are built from the assertion constants rather than retyped, so they cannot drift, and every term is checked lowercase because the OCR output is lowercased before comparison.

**A fixture must not sit where something other than the feature would answer for it.** The preview-link smoke's source and target live in a subdirectory, not at the project root, because the captive Neovim resolves a relative path against its working directory: with both at the root, a missing base URI resolves the raw href to the same file the correct base does, and the smoke stays green with the feature reverted. Moving either fixture back to the root silently disarms it, which is why a guard asserts the subdirectory.

**A red-run demonstration is a deliverable, not a formality.** A check can be aimed at the right property and still look somewhere the defect cannot appear: a press-and-release at two different cells enters visual mode by itself, so a drag assertion needs a round trip whose press and release are at the same cell to discriminate drag events at all; a wrapper that refuses to exec is never named `nvim`, so a child-name scan cannot see an uncapped retry loop; and a centered button spans the same midpoint whatever width it requests, so a center-derived click point is blind to a width regression. Only running the revert exposes any of them.

**A scenario ends at its first failed readback, so a falsifier's red leg is measured, not predicted.** A scenario exits as soon as a readback it waits for fails to appear, and the Rust side stops at its first failing assertion, so later legs are never evaluated and an edit aimed at one leg can go red at an earlier one. A falsifier's leg table therefore names the disabling edit, and the coder records which leg went red after running it.

**Exit codes carry meaning, and folding two causes into one code destroys evidence.** A process that vanishes because the main loop panicked looks exactly like one that shut down cleanly, so "exited non-zero" gets its own code rather than joining "still alive"; likewise "the app died with its Neovim", "not exactly one live child", and "the killed child never left the process table" are each distinct.

**A smoke closes a tab by its close button or a middle-click only once the tab is unmodified.** The close asks the tab's Neovim, which refuses while any of its buffers is modified, so `panes.rs`'s native-tabs smoke and `titlebar_tabs.rs`'s first smoke type `:set nomodified` in the closing tab first.

**A message smoke may press no key after the one that produced the message.** Any `Key`, `Mouse` or `Scroll` is dismissing input for a non-prompt toast in `wolf_core`, and so are the drawer toggle, a focus move into a terminal pane and input to one, so a stray keystroke or chord between the command and the capture clears the surface the smoke exists to read. The completion popup has the same shape for a different reason: the next key closes it.

**No scenario sends input before `wait_ready`.** The environment helper prepends one shell prelude to every scenario and sets `WOLF_READY_FILE`; after each launch the scenario calls `wait_ready "$app_pid" <died-code> "<label>"`, which primes the seat's keyboard with a key-free `wtype ""` and returns once the file holds that pid — the app writes it when its windows accept keystrokes ([wolf-gnome-decisions.md](../../../notes/wolf-gnome-decisions.md#the-readiness-file)). A dead app exits with the scenario's own died-early code, and a launch that never gets ready exits 40 after 30 s; the app's `wolf-ready:` stderr lines then name the condition it was stuck on.

There is no fixed launch budget before `wait_ready` and no delivery margin after it: a key lost past readiness is a missing condition that a margin would hide. A sleep that waits for something else may still follow it, as `TREE_SETTLE` does for the sidebar's file search, and `SETTLE` is the settle between steps.

`every_launch_waits_for_readiness_before_its_first_input` checks the launch side only: no `sleep` between a launch and its `wait_ready "$app_pid"`, that call as the first non-blank, non-comment line after each `app_pid=$!`, and as many `app_pid=$!` lines as launch lines. Nothing mechanical keeps a margin out after the call.

**Every smoke launches in a fresh temp working directory, and that is mandatory.** The captive Neovim keeps a durable swap directory keyed on the cwd, which deliberately outlives the process so a fresh nvim can recover a crashed one's unsaved edits. At a fixed cwd, one run's leftover swap file makes the next run's startup buffer raise a blocking prompt that eats the first keystroke, and the smoke goes red for whatever change happens to be in flight.

**A hung smoke does not fail, it hangs.** A scenario's trailing `wait "$app_pid"` carries no timeout, so a wedged run holds its cage slot indefinitely and the binary never finishes — one `cage` outlived a ~15 s normal runtime by 16 minutes. The other smokes keep running on the remaining slots, though at `WOLF_SMOKE_JOBS=1` everything waits behind the hang. Libtest's `has been running for over 60 seconds` line is a hint rather than proof: it also names a smoke that is only waiting for a slot, or one that is slow under load.

**A failure that appears only when the cages run together is most likely load, not isolation.** Each cage has its own runtime directory, Wayland socket and session bus; what the cages share on purpose is the shader cache in `XDG_CACHE_HOME`, the fontconfig cache and the X11 display numbers. Reproduce it with [`WOLF_SMOKE_JOBS=1`](../../../build.md#building-and-testing) first, then under load, and read the kept runtime directory before touching a settle.

**A smoke that can reach a native `gtk::FileDialog` must export `GDK_DEBUG=no-portals`.** Whenever the app reaches a session bus GTK takes the desktop-portal route, and `cage` implements no `zxdg_exporter_v2` to export a window handle through, so the request goes to whichever portal that bus offers. Each cage's bus is private and activates no portal, and the export keeps GTK's in-process chooser, the one the smokes were measured against, without resting on that; if the isolation ever regresses, it is what stops a modal file dialog opening on the developer's own desktop and hanging the smoke.

`no-portals` forces GTK's in-process fallback, which was measured to present under `cage` and to answer its callback. It is per-smoke scenario environment and never production behaviour, and it belongs on every smoke whose red path could open a dialog, as well as on every smoke whose green path does.

**A palette-driven smoke depends on a ranking it cannot see, and one such dependency is live.** `prv` matches `Preview`, `Preview in Browser` and `Preview in Another Pane` alike, and no query can separate them: `rank_matches` is subsequence-based and the first title is a prefix of the other two. The empty-MRU tie-break puts `Preview` on top, but `mru_group` leads the sort key, so a command MRU carrying `preview_in_browser` inverts the order and Enter would hand the document to a browser instead.

The preview smokes are safe only because each run gets a fresh `WOLF_STATE_DIR` and none of them ever activates that command. `palette::tests::palette_rows_for_the_preview_smoke_rank_the_named_command_first` asserts the inversion rather than hiding it, so a tie-break change reddens there instead of silently retargeting a smoke.

The other two preview queries carry no such dependency: `prvan` and `ednap` each match exactly one title in the whole registry, so they rest on uniqueness rather than on a ranking. Every smoke needing the preview beside its source types `prvan`, because plain Preview binds into the active pane and from a one-pane window would leave no editor half for a control band to sample.

**The rendered page must be identical on every run, not merely dense.** The temp cwd is a unique parent holding a fixed-name child, and the helper hands the parent to the app as its home directory, so the title seam abbreviates the varying part away and every header bar renders the same string. A run-varying title makes an unrelated, correct smoke stochastic. Sparse pages have the opposite failure: an isolated leading glyph is misread as a ligature, which is why several smokes drop filler files purely for density. If that regresses, add filler files rather than weakening an assertion.

**The block cursor corrupts the cell it covers, and the default page-segmentation mode reads nothing here.** A new scenario parks the cursor off the asserted row with a normal-mode `o` then Escape, because the inverted block turns the glyph beneath it into noise, and keeps the sparse-text segmentation the helpers pass, because the default mode reads nothing at all on the light-on-dark grid.

**A terminal marker must not share a row with the shell prompt.** Resizing a pane, whether by a drawer opening below it or a split halving it, makes readline redraw its prompt row from column 0 and clear it, erasing a marker printed there. End the marker's line, as `; echo` does, so it sits on a row of its own.

**A colour claim needs pixels, not OCR.** `tesseract` cannot tell light-on-dark from dark-on-light, so no OCR assertion is evidence about a colour whatever its page says. A colour smoke decodes its own `grim` capture with `cairo::ImageSurface::create_from_png`.

Its sample band must be derived from the pane-area constants rather than hardcoded, and must be paired with a control band in the counterpart pane. Without the control, a coincidence between the editor's own background and the theme's would satisfy the assertion.

**No pointer smoke hardcodes a pixel inside a pane.** The cell size is measured through the same metrics path, font size and line height the app is launched with, one probe click establishes the origin, and every later target is expressed as a delta from it. The header bar's [toggle group](#smoke_headlesssidebar_togglesrs) and the search panel's [toggles](#smoke_headlesssearch_togglesrs) are the exceptions, clicked at targets built from libadwaita's metrics, which their smokes' entries say is safe there. The derived pane origin is checked as a loud, self-checking precondition whose failure message names the candidate causes in order.

What a green run establishes is band membership, not equality: the derived origin need only land within `PANE_ORIGIN_TOLERANCE_PX` plus one cell, a band wider than a cell, so it cannot tell a stale constant from a current one. `PANE_ORIGIN_X_EXPECTED` and `PANE_ORIGIN_Y_EXPECTED` are bit-identical to an origin derived from a solo pointer-smoke run: its persisted probe readback from the smoke's temp working directory, crossed with a cell measured through the same `CellMetrics::for_font` path the app uses. Any change that could move a pane's chrome owes that derivation by hand, not a green tick.

**Two origins exist and neither may be derived from the other.** A probe click reports a Neovim cell, so what it establishes is the text grid's corner; the crash overlay is anchored to the pane area's corner, which is measured its own way and carries its own provenance. The per-pane chrome is the gap between them, and a click derived from the wrong one lands a whole statusbar strip and padding low — outside the button it was aimed at. Re-deriving the geometry after the run does not catch that: the re-derivation recomputes from the same anchor and moves with it, so both sides stay self-consistent while the click misses, which is the vacuity shape the red-run paragraph above collects.

A divider handle is pressed at a modelled position when it belongs to a single `gtk::Paned` whose span is known: anchored at the pane area's corner, `OVERLAY_ORIGIN_X_EXPECTED` or `OVERLAY_ORIGIN_Y_EXPECTED`, with each child floored at `editor_inset::min_pane_size` and the handle at a known fraction of the span. `zoom_pane.rs`'s `tree_handle_x` and `terminal.rs`'s `drawer_strip_edges`, which `drawer_divider.rs` uses, model it so. The press lands because GTK's grab area reaches several pixels either side of the one-pixel separator Adwaita draws. Such a smoke's drag precondition names a missed press as the first cause in its failure message.

The [`pointer.rs`](#smoke_headlesspointerrs) divider smokes bisect instead. Their model starts from `PANE_ORIGIN_X_EXPECTED`, the text grid's corner rather than the pane area's, and cannot see each handle's width along a chain of dividers, as the comment above `BISECT_SEED_MARGIN_PX` says; so the positions it derives are seeds only, and the smoke bisects clicks against the window Neovim reports as current. Before each probe it clicks at the current low bound, because a click that lands on the handle changes no window: without that reset the outcome reads as whichever pane the previous probe focused, and the search converges on the wrong side.

**A smoke resizes the window only by resizing the output, and injects no pointer event after it.** `cage` maximizes every parentless toplevel to the output layout, on map and on every layout change, so nothing gives the window a size of its own; `main.rs`'s `RESIZE_OUTPUT_FRAGMENT` sets a custom mode on the headless output with `wlr-randr` and waits for the listing to call it current. Every pixel model above, and the pointer helper's extents, hold at the launch size only, so a press after a resize lands on whatever is there instead.

**Under `ext_multigrid`, a width readback must use `winwidth()`.** Resizing a grid changes the window's grid width, which `winwidth()` reports, and leaves its layout width, which `nvim_win_get_width()` reports, untouched. A readback built on the latter is blind to a divider drag — it reports 22 and 22 cells either side of a drag a screenshot shows plainly — and it drifts on its own across a relaunch.

**Where the smokes' durable state and configuration go is asserted, not assumed.** The environment helper redirects both directories, and the relaunch smoke checks each in the same four parts: that the test process has no such variable of its own, that the smoke's own directory holds the file, that neither home-derived fallback directory does, and that the developer's real file is byte-identical across the run — including the normal case where it does not exist and must still not exist afterwards, which carries the most weight for the config, since `load_or_create` creates the file it does not find.

**Every smoke is given its own `WOLF_CONFIG_DIR`, and it is the loudest of those redirects.** Without it `wolf_core::preferences_dir()` falls back to `XDG_CONFIG_HOME`, which the environment helper points at the smoke home's empty `.config`, so a smoke would have a `wolf-config.yaml` written for it carrying `iA Writer Mono S` at 10 pt, a family the pinned fontconfig resolves, and every glyph in every smoke would change at once.

The directory is a sibling of the working directory rather than a child, for the same reason the state directory is, and it is seeded with a config naming the pinned family, size, measured `line-height` and `appearance`, so `load_or_create` reads rather than writes. The one smoke that must let the config decide the font size removes `WOLF_FONT_PT` from its own command after the helper has built it.

**The seeded `appearance` is what makes every OCR page host-independent, and re-pinning it to `auto` would undo that.** Every smoke forces its appearance, so the resolved answer is the forced one whatever the desktop reads, and the chrome flip was exercised from both starting points: the full target is green on a host whose chrome starts dark and under a forced light start, with a probe confirming `is_dark` false before `set_color_scheme(ForceDark)` and true after. An `auto` pin would hand every OCR page back to whatever colour scheme the machine running them happens to prefer.

**The seeded config carries no `gtk-titlebar-style`, so every smoke runs under the default `tabs` unless it pins `native`.** Under `tabs` a one-tab window is unchanged, and a window of two tabs or more keeps the one-tab pane origin, `PANE_ORIGIN_Y_EXPECTED`, which [`titlebar_tabs.rs`](#smoke_headlesstitlebar_tabsrs) checks. Under `native` a two-tab smoke's pane area starts at `OVERLAY_ORIGIN_Y_EXPECTED + TAB_BAR_HEIGHT_EXPECTED`.

A smoke pins `native` through `main.rs`'s `titlebar_pinned_config_dir` when, with two tabs or more open, it clicks or reads the tab bar's row, or asserts a size or position that includes it, because `tabs` moves that row into the header bar. These pin it:

- every `titlebar.rs` smoke, through `smoke_config_dir_at`'s style argument, since each also forces its `appearance`;
- `close_tab.rs`'s `close_tab_asks_the_tabs_neovim_under_cage`;
- both `close_window.rs` smokes;
- `tab_bar.rs`'s `each_tab_is_titled_by_its_own_directory_under_cage`;
- every `new_tab.rs` smoke, through `run_new_tab`;
- `panes.rs`'s `native_tabs_open_switch_and_close_under_cage` and `a_file_saved_in_one_tab_reloads_in_another_on_switch_under_cage`;
- `drawer_divider.rs`'s `drawer_divider_drag_resizes_and_is_remembered_under_cage` and `each_tab_keeps_its_drawer_share_across_a_relaunch_under_cage`;
- `extras.rs`'s `pane_cursors_are_captured_and_restored_under_cage` and `each_tab_keeps_its_sidebar_page_under_cage`;
- `terminal.rs`'s `neovim_panes_stay_visible_beside_terminals_under_cage`;
- `titlebar_tabs.rs`'s `native_keeps_the_tab_bar_row_under_cage`, whose subject `native` is.

The multi-tab smokes of `recovery.rs`, `restore.rs`, `file_open.rs` and `open_folder.rs` assert on readbacks and saved state alone, so they keep `tabs`.

## Pointer injection

Real pointer events reach the GTK controllers through [`wolf-pointer-inject`](../../../tools/wolf-pointer-inject/README.md), this repo's own long-lived virtual-pointer client, built by `flake.nix`. Its README owns the reasons it must be long-lived, why the off-the-shelf tools cannot work, and its command grammar; do not restate them here.

Every smoke that injects a pointer event uses it, among them the crash-overlay one, which needs a real click on the restart button, the preview-link one, whose click is the only injected point aimed at something WebKit drew, and the native-tabs one, whose middle-click is the only injected button-2 press. Each starts the helper inside `cage` before the app, so the seat already carries pointer capability when GTK binds, waits for its readiness line, and then drives it over a FIFO with a poll-for-ack protocol: an ack means the compositor has processed the event, not that the bytes left a buffer.

The scenarios hold the FIFO's write end open on a dedicated descriptor for their whole lifetime, since each write would otherwise close it and hand the helper a spurious end of file, and a trapped cleanup kills the helper before the app so shutdown never races an event.

**In-gesture commands are paced, and the pacing is load-bearing.** Neovim's mouse API transports only the button code through the typeahead while the position rides on globals the handler overwrites, so three calls landing a fraction of a millisecond apart all resolve at the last position. Pacing also stops GDK's motion compression folding intermediate waypoints into one drag update. If a drag smoke flakes, raise the pacing interval; never touch an assertion.

One diagnostic trap is worth writing down: libwayland's client-side debug output prints `interface#id.event` with a `#`, not `@`, so a grep for the wrong separator matches nothing and reads exactly like "no events were delivered".

## What is still unproven

Being scrupulous about this is the point of the section.

- **The strict press ordering.** That a click focuses the window is proven, because closing the clicked pane closes the right one. That the focus change is sent before the press is not observable, since Neovim's own left-mouse handling also moves the current window. It is guaranteed by construction — both ride one ordered channel — and must not be ticked.
- **The recorded pointer position.** The scroll handler names its target grid explicitly, so a stuck recorded position would still scroll the right window and the smoke would still pass. Nothing asserts on what the motion controller produces.
- **The press-pinned grid and button.** No smoke drags from one pane into another, so there is no other grid for a wandering drag to be retargeted to and the pin is never discriminating. Proving it needs a split, a press in one pane, and a release in the other.
- **The strip's bold reaches no pixel.** The page-click smoke in [`smoke_headless/preview.rs`](#smoke_headlesspreviewrs) reads which strip is active from raw pixels, as the inactive primary line's ink scaled by `STATUSBAR_INACTIVE_ALPHA` against the active one's, so the dim is observed. That reading sees colour and not weight, and OCR reads neither a weight nor a dim — the same shape as the sidebar's current-file bold below — so nothing observes `primary_bold`.
- **Three further properties of the per-pane status bar reach no pixel.** A clipped path is indistinguishable from a short one, and an overflow would have to be arranged deliberately; no smoke opens an unnamed buffer in a pane, so `wolf_core::pane_status`'s `[No Name]` and empty-display-path arms are table-tested only; and deleting the status refresh on the layout-rebuild path draws `[No Name]` for exactly one frame, which a settled capture cannot see. `statusbar_text` and `draw_pane` are demonstrably read in every case.
- **The inset's composition test discriminates nothing inside the inset itself.** Measured: `cell_at` clamps into the grid and the division never returns a zero dimension, so the property holds for every input the pair can produce, and it reddens only under an edit to `geometry::cell_at`'s clamp. Read it as a guard on that clamp surviving, not as coverage of the inset — the two seams' own tables are what cover the arithmetic.
- **No scrollbar reaches a pixel.** `scrollbar_thumb` and `scrollbar_thumb_color` are table-tested and `draw_pane` demonstrably reads both, but the thumb is an eight-pixel grey mark in a padding gutter and OCR cannot read one: tesseract's `contains` is a word test on a page of glyphs. Nothing observes that scrolling moves the thumb, or that a buffer fitting its window draws none.
- **The cap cannot bind at the smokes' geometry, so nothing reaches a pixel inside a capped, centred grid.** Measured: one pane leaves 989 px of grid where ninety columns of the shared cell need 2022.9 px, so no smoke can see the cap, the centring offset is zero in every one of them, and the toggle smoke chooses its own font size for that reason. `the_column_cap_cannot_bind_at_the_ocr_smokes_geometry` keeps that premise honest rather than assumed, and `editor_inset::a_capped_grid_maps_the_margins_and_its_own_edges_to_real_columns` covers the arithmetic headlessly with literal probe coordinates. The status-bar strip's alignment under a centred grid goes the same way: `grid_rect_table` pins the `x` that `draw_pane` passes to `draw_text_line`, and OCR cannot read a horizontal offset anyway.
- **The palette's focus return.** No test discriminates it: deleting `PaletteInner::close`'s `area.grab_focus()` leaves the command-palette smoke green across three runs, so that smoke's marker assertion is a precondition rather than a discriminator. Why hiding an overlay behaves differently from destroying a focus widget on a pane rebuild is unestablished — only the two outcomes are measured, and the pane-rebuild case reddens `click_then_close_keeps_keyboard_input_alive_under_cage` under revert. Treat the focus rule in [Window structure](../../../notes/wolf-gnome/window-structure.md) as binding for every overlay until someone measures the difference.
- **Arrow navigation in the palette, the buffer picker and the sidebar's search panel, and Escape in the first two.** No smoke moves any of their selections or closes either overlay with Escape; each is written to yield exactly one actionable row, which is what makes its ranking deterministic. The arithmetic beneath all three is the file picker's `next_selection`, with its `restored_index` for the two overlays and `selection_after_rebuild`'s table for the panel, and `PaletteInner::refresh`'s reset-vs-preserve rule under a changing query is a widget closure the fast suite cannot reach either. The buffer picker's row-1 preselection is not part of the gap: the bare-Enter smoke exercises it end to end.
- **Four things about the Open Recent rows.** The three refusals are table-tested only, since the palette closes before `grim` fires and a two-row query would make which row Enter activates depend on ranking tie-breaks. `open_recent_row_is_matched_by_its_visible_label` has no independent revert — the rank/display agreement is the absence of a post-rank rewrite, so it reddens only under the `rank_palette(&[], &[], …)` one. The MRU key shape is `wolf_core::rank_palette`'s, so asserting on it is a join between two crates. And `open_recent_candidates_table` asserts inside a `for` loop, where a revert breaking two rows was measured to produce one observed failure.
- **The ellipsized palette row label never reaches a pixel**: the palette smoke captures once the overlay has closed. The one capture taken with a transient overlay up belongs to `file_picker_covers_the_live_preview_under_cage`, which asserts on pixels and reads no text.
- **The rest of the overlay chrome reaches no assertion.** What that one capture discriminates is the box's own surface and the height of its selected row; `@popover_fg_color`, the 12px `border-radius`, the `box-shadow`, the box's `padding: 6px` and the `entry`/`label` bottom margin are read by nothing. The radius and the shadow are visible indirectly — they are why the sampled rectangle is inset by `PICKOVER_CORNER_INSET_PX` — but nothing asserts they are present, and a build carrying neither would satisfy that inset trivially. Each needs a pixel assertion of its own: OCR reads neither a colour nor a geometry.
- **Light mode reaches no pixel path at all.** Every measurement behind those assertions is Adwaita dark, where `@popover_bg_color` is `#36363a` against `#ffffff` in light, and the one transparency block over `scrolledwindow`, `viewport` and `list` is what makes the box's own surface visible — so light is where a transparency rule that does not hold would show first. The `viewport` selector reaches no assertion even in dark, and that is measured rather than inferred: it changes no pixel there, and [`src/README.md`](../src/README.md#overlay_stylers) records why it is in the block anyway. Closing this needs a second capture taken with an overlay up under a light appearance.
- **No assertion says the overlay's surface is `@popover_bg_color`.** Assertion 4 reads the fill empirically and only rules colours out — the unstyled `(29, 29, 32)` among them — so a stylesheet resolving to some other opaque colour passes. Measured over the two breakages taken: emptying `overlay_style::apply` reddens that inequality, while mistyping `@popover_bg_color` reddens the absence half instead, the transparency block having survived the erroring declaration. That inequality is a `!=` against the libadwaita colour `PICKOVER_UNSTYLED_FILL` pins, so a release re-tinting `@view_bg_color` degrades it to a no-op silently, and no assertion can watch that: reading the colour out of the running theme compares it with itself.
- **`PaletteInner::open`'s no-core path.** Empty candidates, with the palette still opening on the registry, is widget-bound and unreachable from the fast suite.
- **The buffer picker's modified marker never reaches a pixel.** `MODIFIED_SUFFIX` is table-tested in the seam, but no smoke opens the picker with a modified buffer, and the marker glyph reads poorly through OCR in any case.
- **Every overlay refusal, and `BufferPickerInner::open`'s ordering.** A refusal returns before any widget is touched, so `buffer_picker::open_decision`, `search_panel::show_decision`'s no-project arm, `search_panel::activation_route`'s and `nav_ui::nav_press`'s `Refuse` arms are invisible to both suites; the seams' tables cover the whole domain, and no smoke can arrange a dead, stalled or absent core behind a chord press or a result. That nothing is shown until the candidate snapshot returns is widget-layer for the same reason, and is written down in [`src/README.md`](../src/README.md) so nobody tidies the show-after-fetch ordering back.
- **The search panel's `WOLF_RG_BIN unset` status and its row windowing.** `search_status` is table-tested in the seam, and [`search_toggles.rs`](#smoke_headlesssearch_togglesrs) reads the stale, no-matches and rg-error lines, but every smoke launches with `WOLF_RG_BIN` set. The smokes' match lines also fit a row whole, so no capture shows a line windowed to `char_budget` or a Pango ellipsis, and nothing re-windows the rows after a divider drag.
- **The in-flight cancellation on a second Enter.** Nothing observes a killed `rg` child.
- **`palette::every_registered_command_is_classified_by_the_palette` has no red run of its own here.** Measured: it stayed green under the revert that restores `Command::SearchInProject` to a `Deferred` classification.
- **`search_panel_streams_results_and_opens_the_match_under_cage`'s assertions 3, 4 and 6 have no isolated falsifier.** Assertion 3 says the Search page replaced the tree, 4 that no overlay drew the results, and 6 that the pane re-rendered onto the target; the smoke's table gives, for each, why every edit reaching it fails an earlier assertion first. Assertion 3's anti-vacuity is the search-chord smoke's assertion 4.
- **`window::wire_keys`' buffer-picker guard.** Deleting `|| buffers.is_open()` leaves both buffer-picker smokes green across a run. The clause stays, and the file picker's and the palette's clauses and the search panel's `owns_keyboard` clause in the same guard are equally undiscriminated. No mechanism is offered for that measurement, because none was measured.
- **The overlay guard on the commit path.** `im_ui::commit_decision`'s `Refuse` arm is unreachable from the shipped glue: GTK's built-in context emits `commit` synchronously inside `filter_keypress`, and `window::wire_keys` makes that call only where no overlay is open. Reaching it needs an out-of-process engine, which the harness cannot run at all — the flake ships neither an engine nor a GTK 4 ibus module. Table-tested, plus [`im_commit_nvim.rs`](#im_commit_nvimrs) driving the send loop over a refusal; the same shape as the buffer-picker guard above.
- **A commit arriving after the close handler has taken the core out.** Only an asynchronous engine can deliver one, so the commit handler's `if let Some(core)` arm is executed by nothing. What it holds is the teardown ordering: `focus_out` on a context with a pending sequence may commit it, and the arm is what makes that arrival perform nothing.
- **The release forward.** Nothing observes that the input method is offered a release, or what becomes of one it claims — and nothing could, because GTK 4's `key-released` is a void signal: there is no propagation to stop, so a claimed release has no effect to distinguish. The forward exists because `set_im_context` offers both phases and the hand-rolled call was licensed by a measurement of that equivalence. A wrong release path is loud instead of hidden — translating one sends a second copy of every key, which every OCR smoke reads as doubled text.
- **`apply_overlay`'s `buffers.close()` has no falsifier.** The buffer-picker box sits below the opaque stall overlay in the stacking order, so an unclosed one is invisible to the capture and the stall smoke's assertions do not depend on it. What the call prevents is an overlay left flagged open, which keeps `window::wire_keys`' guard suppressing every key to Neovim, including after a later restart; nothing smokes that.
- **The bare-Enter smoke's must-not-contain assertion has no isolated red run.** No revert can put two buffers' headings on screen in a single pane, so the assertion is reached only once an earlier one has already failed. What it contributes is naming which wrong row was activated, not detecting that one was — the same shape as the crash assertions below.
- **The `Ctrl+e → None` accelerator row is covered collectively rather than individually.** Any revert that breaks the shift-exactness of the chord matcher breaks it for all ten bound letters at once, and the table asserts in one sequential loop, so an earlier row fails first. The invariant has two independent red runs; this row has none of its own.
- **The `Ctrl+c → None` and `Ctrl+v → None` rows inherit exactly that caveat**, and they carry more weight than the others, because `<C-c>` interrupts and `<C-v>` is visual-block and insert-mode literal-next. What closes the gap is a measurement rather than a row: against the built binary under `cage`, Neovim in operator-pending with `d` survived a plain Ctrl+C and a following `w` with line 1 intact, and plain Ctrl+V then `097` in insert mode produced the single byte `a`. Neither is a test, so neither can regress loudly.
- **The one-overlay-at-a-time rule.** No smoke opens one of the three transient overlays while another is up. It is two idempotent `close()` calls per overlay arm in `shell::perform`, and three in the search panel's.
- **The stall overlay's Restart button on a wedged core.** The stall smoke captures the overlay but clicks nothing, and the crash smoke's click is against a dead core — so the one route out of a stall is unexercised end to end.
- **The `NotResponding` to `Alive` transition.** No smoke drives it. Core's input pump makes it once Neovim answers a call or a probe it was waiting on, which a smoke reaches only by typing into a wedged Neovim and waiting out the wedge. `crates/wolf-core/tests/nvim_liveness.rs` covers the mechanism against a genuinely wedged Neovim.
- **Showing a window's size**: a restored one, at launch and on an Open Folder fill of a lone tab, and a new window's [§7](../../../specs/application-state.md#7-new-windows-and-tabs) size. `cage` maximizes every toplevel to the output, as [Smoke conventions](#smoke-conventions) says, so no such size takes visible effect, and a smoke reads one only from the save, which writes the window's default size. Resizing the output sets the window's size rather than showing its own; that needs a compositor that lets a toplevel keep its size, such as `sway` with a floating rule, which nothing here runs. Window position is no gap: it is deliberately never persisted, per [Application state](../../../notes/wolf-gnome/application-state.md).
- **That a new window opens unmaximized.** `cage` maximizes it too, so nothing here can see it. It holds by construction: a `RestorePlan` carries no maximized state and `window::build` requests none.
- **A drag on a horizontal (stacked) divider.** Both drag smokes drag a vertical one. `set_divider_fraction` never reads a split's orientation, so what a stacked drag would add is the widget side: the orientation mapping into a vertical `gtk::Paned` and that paned's own span, which is its height rather than its width.
- **The `max` and `span` arms of the allocation guard on `notify::position`.** The resize smokes prove the guard's last-position refusal: deleting it turns both red, the drawer smoke's and the zoom smoke's. Deleting the `max != last_max` and `span != last_span` tests from `DividerGuard::drag_span` leaves both green, because GTK's queued `position` notify runs after the `max-position` re-apply has re-armed them, as [pane weights and dividers](../../../notes/wolf-gnome/pane-weights-and-dividers.md) says. So no smoke tells those arms from no check, and the notify they refuse, one ahead of the re-apply, is reasoned rather than measured.
- **The drawer's own floor under a downward drag.** The divider smoke's limit drag goes up, against the editor's floor. `position_table` in [`drawer_share.rs`](#drawer_sharers) pins the clamp at the drawer's.
- **A share dragged in one tab reaching a drawer already open in another.** The wake's re-apply in `apply_drawer_frame` exists for it, and no smoke drags with drawers open in two tabs: the divider smoke's second tab opens its drawer after the drag, which the new paned's own `max-position` notify covers.
- **A dragged share surviving a restart.** No smoke restarts Neovim after a drag. Resetting the share in `clear_drawer`, the path a restart shares with a closing drawer, turns `drawer_divider_drag_resizes_and_is_remembered_under_cage`'s A4 red; a reset elsewhere on the restart path would pass.
- **That `set_active_pane` records the most-recently-used editor pane.** Measured: deleting the `record_active` call leaves every assertion green, because the MRU and `bottom_most_leaf` answers coincide wherever the drawer smoke issues a `<C-w>k` — one editor leaf before the `:vsplit`, and after it `:vsplit` focuses the new left window while `bottom_most_leaf` ties on the bottom edge to the smallest `x`, also the left. The failure mode is where `<C-w>k` lands, cosmetic rather than a trap, which is why it is recorded and not converted. The recipe is kept so a later unit can take it cheaply: after the `:vsplit`, `<C-w>l`, `<C-w>j`, `<C-w>k`, reading back `winnr()` — green answers the right pane, red the left.
- **Where the drawer strip sits after a rebuild.** The drawer smoke's leg f proves only that the drawer's binding survives a split's rebuild, and `clear_drawer()` in `apply_split` turns it red. No capture follows the `:vsplit`, so where the rebuilt strip lands reaches no assertion.
- **The drawer's own status bar reaches no assertion.** Its OCR band is sampled below the statusbar deliberately, the region measured byte-identical across runs, so nothing here reads the shell's name the statusbar shows; `wolf-ffi`'s [`pane_status.rs`](../../wolf-ffi/tests/README.md#pane_statusrs) asserts that string. The same shape as the per-pane status-bar entry above.
- **The snapshot gate.** Its whole domain is table-tested, but no smoke arranges a wake in which Neovim is dead, or no pane is bound, and then checks that the layout saved before it survived. Inducing that race under a compositor would be timing-dependent rather than deterministic.
- **The Switch to Window raise, and Open Folder's and a request's.** `Coordinator::raise` calls `present()`, and an Open Folder focus or fill and every request raise through `WindowHandle::raise`, which `cage` declines: it shows one toplevel at a time and keeps the newest on top, so no capture can show two windows and nothing can bring a backgrounded one forward. The palette's rows are proven as values by `palette.rs`'s tables and `Coordinator::raise` is executed by no test at all; an Open Folder focus is observed only as the tab it selects, and `wolf` alone, which selects and raises the current tab, by nothing. Closing the gap needs a `wlr-foreign-toplevel` client, which is a `tools/` deliverable of its own.
- **`close_commit`'s `DropSubtree` arm.** No smoke closes one of two windows — and on the last window the arm is not merely unobserved but indistinguishable from a no-op, because an empty contribution set means "commit nothing" rather than "commit an empty state" and so cannot write `windows: []`. The rule is covered by its table in `state_seam.rs`.
- **`close_commit`'s `Skip` arm.** The store no-ops on an identical tree, so a redundant commit on the quit path writes nothing and no observable a smoke has can tell it apart from the correct answer. Same table.
- **That the close path is what saved the session, rather than the last per-wake commit.** Measured: by the time `:qa` reaches the drain a per-wake commit has already written an identical tree, so the close path's commit and flush are redundant on every existing scenario and no smoke discriminates them. The redundancy is pre-existing rather than introduced. What did establish the routing is a two-step falsifier: disabling the per-wake commit alone leaves both relaunch smokes green, and adding the close-path revert on top reddens both.
- **The single-instance second `activate`.** It is reachable: a second `wolf-gnome` launched inside a cage while the first is alive hands off to it on that cage's private bus. No smoke does that, so a desktop's "launch it again, get a window" path is unexercised; the two-window smoke's subject is New Window, which a hand-off would bypass.
- **`Coordinator::perform`'s catch-all arm.** Which rows are `app.`-scoped and which `UiAction` variants `perform` accepts are two hand-maintained lists, each pinned by its own test; if both drifted the same way both tests would stay green. A third hand-maintained table over the accepted set does not close a drift-in-lockstep of two, so the hole is recorded instead.
- **The button-width check discriminates only a large regression.** The button is centered, so the left-inset click point misses only below roughly three-quarters of the requested width, and the button's height is undiscriminated entirely.
- **Two crash assertions have no isolated red run.** The overlay-present assertion cannot be reached by a revert that hides the overlay, because the button lives inside it. The unsaved-edit assertion goes red alongside everything else on a broken restart; what it independently discriminates is that the reopened buffer came from the swap file rather than a clean re-read from disk.
- **Forward is never driven end to end**; only Back is. `nav_press` and `nav_buttons` treat the two symmetrically and core's `history_forward` mirrors `history_back`, so what stays undiscriminated is the widget wiring for Forward alone.
- **Nothing asserts that the navigation buttons follow the focused pane at the widget level.** The rule is table-tested in the seam, and OCR cannot read a sensitivity state in any case. Related and equally unmeasured: `refresh_nav_buttons`' deadlock hazard — a read guard held across the nav mutex would hang on a wake, indistinguishable from a slow machine.
- **The restored `sidebar.visible` reaches no pixel.** No relaunch smoke hides the sidebar, quits and relaunches. The seam is table-tested and the applier demonstrably reads it; the write side is the half that could regress silently, since persisting the live visibility rather than the preference would hide the sidebar after any project-less session.
- **The per-tab sidebar preference and empty state follow the selection in no smoke.** None toggles the sidebar in one tab and selects another, and none selects an empty tab beside a folder tab, so `apply_selected_tab` on a switch and `settle_reroot`'s empty-tab guard rest on the code alone.
- **Minimize, in either direction.** `cage` ignores a minimize request, so no smoke reaches `on_window_minimized`, the `minimize()` before `present()` of a window saved minimized, or a minimized window saved last; the minimized class is [`stacking.rs`](#stackingrs)'s and [`app_state_seam.rs`](#app_state_seamrs)'s tables alone, and so is a minimized window's present at once.
- **Neither the current-file bold, the revealed row's selection nor manual expansion reaches a pixel.** OCR reads no font weight, no highlight and no expansion state, so the bound-label registry — the only thing keeping the highlight on the file being edited while the version gate suppresses the rebuild — is unverified in both suites, and every assertion about the current file reads a revealed row's presence rather than its weight. The user-visible payoff of the version gate goes the same way, as does its inverse, a collapse on a version advance, which is recorded as a limitation in [Present limitations](../../../notes/wolf-gnome/present-limitations.md) instead.
- **The reveal tick's withdrawal while the tree is unmapped, and its `map` re-arm.** No smoke hides the sidebar between a reveal and its scroll, so nothing observes that the frame clock goes idle or that an owed scroll runs on the next show. Measured: deleting the re-arm leaves `tree_reveal.rs` green, because the launch reveal runs with the list mapped ([The sidebar reveal](../../../notes/wolf-gnome/the-sidebar-reveal.md)).
- **The visibility gate's deferral and its one catch-up.** No smoke hides the sidebar mid-traversal and re-shows it, so nothing observes a batch that landed while hidden reaching the widgets. The gate's own table pins the rule; the relay from the shell's single visibility site is the untested half.
- **That the coalescer arms a real `glib` source.** Only the pure state machine is tested, and every smoke asserts on a settled end state, so a coalescer that performed its refresh on every request — arming nothing at all — would pass every one of them.
- **A configured font family reaches no pixel.** No smoke changes the family: doing so would change every glyph and put the whole OCR calibration at risk for no added confidence, since [`paint_golden.rs`](#paint_goldenrs) covers family resolution headlessly. What a smoke does prove about the config is the font size, through two launches whose `winwidth(0)` readbacks differ.
- **A configured line-height reaches no pixel either.** Its arithmetic is unit-tested and `cell_metrics_for_smoke` demonstrably feeds it, but no capture reads a cell height — and the pinned value is the one that reproduces the glyph box exactly, so a capture could not tell it apart from no line-height at all.
- **Nothing on the toast's scroll path reaches an assertion.** No smoke can produce a message longer than the budget — half the space above the strip, many lines at the smokes' geometry — so the offset never moves, and the widget-side rules under it are unreachable for the same reason: that the gesture re-places the surface before queueing the draw, and that it accumulates against `mouse::scroll_threshold(unit, cell_h)` so a `ScrollUnit::Surface` device's sub-line deltas park a carry. Nothing observes that the toast consumes a scroll rather than forwarding it either: the surface sits above the pane areas with its own controller, and `wheel_scroll_moves_the_viewport_under_cage` scrolls where no toast is showing. Every arithmetic seam beneath is table-tested.
- **A grid-anchored completion popup.** The popup smoke drives cmdline completion, where `Pum::grid` is `-1`. The insert-mode branch — the one that reads a pane's `compute_bounds` and `editor_inset::grid_rect` — is exercised by nothing, so its anchor arithmetic is unmeasured end to end.
- **The popup's selected-row reverse video.** OCR reads neither a fill nor an inversion, the same shape as the status bar's active treatment. `pum_selection` and `pum_view` are table-tested and `draw_pum` demonstrably reads both.
- **That the toast and the popup paint on their cells.** That `draw_toast` and `draw_pum` draw through `render::draw_cells` rather than as one run is unobserved end to end: at the smokes' 28 pt, DejaVu's advance rounds down, so a run is narrower than its cells and nothing clips either way. [`cell_text_paint.rs`](#cell_text_paintrs) pins `draw_cells` itself.
- **The re-prompt path of a `confirm()` dialog.** The confirm smoke answers with a valid key. An **invalid** key, which makes Neovim re-emit a matching `cmdline_show` and core restore the stashed messages, is covered by `wolf-core`'s own `cmdline_show_matching_reprompt_restores_stashed_confirm` and by no smoke.
- **The confirm smoke's third assertion has no front-end falsifier.** `wolf_core::redraw::cmdline_hide` does a `mem::take` on the toast's messages, so the second capture's toast is empty by either route, and `draw_toast` caches nothing to keep drawing from. What that assertion states is a no-caching property rather than an active dismissal — the Linux half of the feat-list row is that this front-end adds no dismissal of its own — and its only genuine falsifier, core failing to take the messages, lives in `wolf-core`.
- **`prompt_answered`'s only production effect is observed by nothing.** All it drives is resetting `toast_scroll` and `toast_lines`, and no assertion reads either; its unit test asserts a two-term boolean against itself, which holds whatever the caller does with the answer. Discriminating it needs a toast scrolled back, a prompt resolved, and the next message read from its newest line — unreachable for the reason the toast entry above gives, since no smoke can overflow the budget at all.
- **`typed_cd_reroots_the_sidebar_under_cage`'s assertion-4 revert is not isolated.** Measured: with no re-root at all the sidebar keeps its launch root label, so assertion 4 fails before assertion 6 is evaluated and assertion 6 has no red run under that revert. The asymmetry is inherent rather than a fixture weakness — the same shape as the project-search smoke's assertion 6 above — and the other direction is isolated: assertion 6's own revert reddens 6 alone, with 3, 4 and 5 green.
- **That a decoration-free cell builds no `AttrList`.** It is a stated performance decision in `GlyphPainter::draw` with no assertion behind it, and probably unreachable from a pixel test in any case: an attribute list carrying nothing renders exactly what no attribute list renders.
- **That the decorations reach the command-line strip, the message toast, the completion popup and the block-cursor glyph.** All four draw through `GlyphPainter::draw`, the strip and the cursor by way of `draw_glyph` and the toast and the popup by way of `draw_cells`, so it follows by construction and nothing observes it. No smoke is evidence about the decorations at all: the captive Neovim under `cage` runs with no colorscheme, no spell and no LSP, so it emits no undercurl or strikethrough cell in the first place.
- **Nothing proves the clipboard reaches another Wayland client.** A `gdk::Clipboard` can serve a `read_text_async` from its own local content provider when the same client owns the selection, so `copy_and_paste_round_trip_through_the_clipboard_under_cage`'s round trip may never leave the process. Adding `wl-clipboard` would not settle it either: wlroots offers the selection to the keyboard-focused client, and a second client started under `cage` beside the app has no focus. Proving the compositor half needs a focus dance no smoke performs.
- **`shell::copy_selection`'s write-nothing-never-clear branch, and the paste callback's ordering.** That an absent or empty yank writes nothing rather than clearing whatever the user copied from another application, and that the callback upgrades its `Weak` before it borrows the core, are widget-layer facts neither suite can execute. `clipboard_ui`'s tables cover the rules; what is unobserved is the glue carrying them out.
- **Anything a desktop portal supplies.** Every cage's bus activates no portal, so every smoke runs GTK's portal-less path: font and xft settings from the pinned `settings.ini`, the dpi from the app's own pin, and the in-process file chooser. What a real desktop's portal hands GTK in their place is observed by nothing.
- **The titlebar provider's per-window scoping and its removal at close.** No smoke runs two windows on different colorschemes, or closes a window and opens another under its id, so neither `TitlebarStyle`'s class nor `uninstall` reaches a pixel. Every titlebar capture exercises `install`.
- **The empty view's per-tab scoping and its removal at a tab's close.** Only the selected tab's view is on screen, and a selection wakes that tab, so no smoke tells a tab's own `Normal` from the selected tab's, and none closes a tab and checks that its provider is gone. That a background tab's view has its own colour when selected holds by construction alone: each tab applies from its own core on its own wakes.
- **Backdrop reached any other way than through a transient dialog, and the bars after it closes.** The titlebar smokes deactivate the window only by opening the save dialog, and kill the app with it still up. GTK's `:backdrop` state is the same when focus moves to another application, which a person checks on a GNOME desktop.
- **The portal route the save dialog takes on a real desktop.** `the_save_dialog_writes_a_named_file_under_cage` drives GTK's in-process fallback, forced by the `GDK_DEBUG` export the conventions above require, so what it proves is bounded by that widget: the seam's `Choose` answer reaches a real `gtk::FileDialog`, the dialog's `Ok(GFile)` reaches the callback, and the resulting `InputEvent::SaveAs` writes the file. `gdk_display_should_use_portal` answers true wherever GDBus reaches a session bus, so a real session gets a different widget, a different confirmation and a different `GFile` producer, none of them observed here.
- **The overwrite confirmation, and the front-end's `force: true` end to end.** The save smoke's target does not exist, so neither is exercised; driving GTK's confirmation alert from the keyboard is fragile enough that the recipe is recorded rather than written. What covers the effect is the pair in `crates/wolf-core/tests/save_open.rs`, `save_as_refuses_to_overwrite_without_force` and `save_as_with_force_overwrites`, each of which discriminates the other against a real Neovim.
- **`AppShell::save_buffer_as` is executed by no test at all.** Measured twice: replacing `shell::perform`'s `UiAction::SaveBufferAs` arm with a no-op leaves the suite green, this file's default tests included. Save As's only surface is the primary-menu popover, the deliberate gap below, so nothing can reach it; its decision is covered by `save_ui::save_as_press`'s table and the dialog glue it shares with Save is driven by the save smoke.
- **The `auto` appearance arm under `cage`.** Measured on the development host: `adw::StyleManager::default().is_dark()` reported `true`. It tracks the host's own colour-scheme preference, so the reading is host-dependent, and it is constant for a whole run — no smoke can vary it, and none may assert a particular value.
- **The live OS-toggle re-push.** `connect_dark_notify` fires only on a real change of libadwaita's `dark` property, which nothing under `cage` can produce. `observe_os` is table-tested and `apply_at_launch` demonstrably reads it; that the handler is ever invoked is unobserved on this platform, and so are the header bar and the tab bar following the background it re-pushes. The titlebar smokes move the background with a typed `:set background` instead.
- **The GTK version gate under the preview's colour scheme.** Nothing can observe that libadwaita is performing the `gtk-application-prefer-dark-theme` write it makes only below GTK 4.23.1; `preview_document_follows_the_configured_appearance_under_cage` observes the outcome alone. See [wolf-gnome-decisions.md](../../../notes/wolf-gnome-decisions.md#the-previews-light-and-dark).
- **`apply_at_launch`'s step order and its `Weak`.** The preview-appearance smoke above reaches that function end to end, and its `set_color_scheme` call carries a measured red run; the step order and the `Weak` do not. The derivation is what stands in: every smoke forces its appearance, so `observe_os` is false and the notify handler is never connected — which means neither connecting it ahead of `set_color_scheme` nor trading the `Weak` for a strong `Rc` can change any observable the suite has. The reasons each must hold anyway are in [`src/README.md`](../src/README.md).
- **The preview's scroll offset reaches no assertion.** No smoke can read `window.scrollY` out of a web view, so what the JSON body swap buys over a full `load_html` rests on the measurement recorded in `load_preview_document`'s doc comment in [`pane_view.rs`](../src/pane_view.rs) rather than on a test. What the live-edit leg does catch is the single-encoding defect, which freezes the document while looking perfect on the first load.
- **Which face the WebProcess uses.** Measured: both bundled `@font-face` families load and are the used faces, the decisive A/B being deletion of the two rules from the `CSSOM` — an `Inter` span moved 624.34 → 662.20 px and a `Latin Modern Math` span 598.58 → 662.20. Three routes to a test were measured and rejected: OCR reads no font; `document.fonts.check()` returns true for both families with the rules deleted, so it discriminates nothing; and WebKit's mandatory sandbox does not bind the devShell's `FONTCONFIG_FILE`, so a comparison against a host fallback is host-dependent. The two families are reproducible because they win over system fonts by spec.
- **The pinned fontconfig does not reach the preview's web process.** Measured: only cages holding `webkitgtk/`, the WebKit smokes', log `Fontconfig error: Cannot load default config file: File not found: <TMPDIR>/wolf-fonts.*.conf`, though the file exists on the host. The inference is that the web process's sandbox cannot see the nix-shell `TMPDIR`, so whatever the preview resolves through fontconfig rather than its bundled `@font-face` families comes from the default configuration. The smokes pass regardless, and nothing that OCRs preview text may assume the pinned fonts.
- **`file_picker_covers_the_live_preview_under_cage`'s ink floor has no captured red.** Its control half demands both a floor of the document's background and a floor of its heading's ink inside the overlap rectangle, and the falsifier written for it — dropping the Preview leg — reddens the window-list readback four assertions earlier instead. Reaching the ink floor needs `WolfCore::render_preview` blanked with the pane still bound, a `src/` edit; until one is taken the floor rests on its green reading and on the background floor beside it.
- **The all-preview tab.** `snapshot_readiness` counts previews as well as `win_buffers`, which is what keeps persistence running for a tab whose every window is a preview. Reaching that shape needs three panes converted in turn and the preview smoke has two, so the integer is unguarded.
- **A fragment scroll's effect on the rendered page.** `scroll_into_view_js` is table-tested and the same-document and cross-document callers demonstrably issue it, but a scroll offset is invisible to OCR for the reason the preview scroll-offset entry above gives, so nothing observes that the page moved — or that it moved to the right element.
- **`document_update`'s choice is invisible to the preview-link smoke.** Measured: forcing it to `InPlaceEligible` for every render leaves `preview_link_click_repoints_the_pane_under_cage` green, because Neovim rebinds the pane either way and the readback is what the smoke asserts on. Its only falsifiable evidence is the unit table in [`preview_ui.rs`](../src/preview_ui.rs).
- **The three rules behind the pending cross-file fragment.** Writing the slot unconditionally on a successful repoint, clearing it in the remove branch beside `loaded` and `head_len`, and taking it rather than reading it on load are reachable only through GTK signals, and are asserted in prose alone. They are where the design's "a stale fragment applied to the wrong file" hazard lives, and one narrow case is live: a clicked path lexically unlike the source but rebinding the same buffer leaves `needs_reload` unfired, so the slot is written and no load edge consumes it. Extracting a pure seam for the write rule is a real option and a later unit's.
- **`any_preview_due`'s short-circuit reaches neither suite.** That a wake with no due pane arms no timer at all is invisible to the pure seam, which never sees the call, and to every smoke, which asserts on a settled end state a build arming one per wake reaches just as well. What it prevents is a `glib` source armed on every wake that moves nothing a preview is bound to.
- **That the glue honours `RenderDecision::Render` inline is unobservable end to end.** Neutering that arm leaves the document arriving one `PREVIEW_TRAILING_DELAY` later through the next wake's `Arm`, well inside the preview smoke's settle window, so every assertion stays green. The leading edge's only falsifiable evidence is `preview_coalescer::tests::the_first_edit_after_an_idle_gap_renders_immediately`.
- **The held-tab deferred fire is reasoned, not measured.** A tab switched away mid-burst keeps its armed fire, which performs after the switch and bypasses the tab gate deliberately; the argument is over the gate's inputs, in the shape of the tabs selection gate below. Nothing arranges a background tab whose burst is still pending.
- **`classify`'s `about:` rows are unmeasured and cannot be smoked.** Their document exists only for an empty source path, which `runtime/nvim/init.lua`'s `wolf_preview` filter makes unreachable from the running application. They rest on WebKit's documented `load_html(content, NULL)` default alone, and the source marks them unmeasured.
- **The external link's browser open.** Measured 2026-08-06 under `cage` through temporary `eprintln!`s in `preview_in_browser` and `launch_uri` that were **never committed**: `open_in_browser` answered `http://127.0.0.1:33393/alpha.md`, `launch_uri` was called with it, and its callback answered `Ok(())`. No failure line was printed because there is no failure to print, which is why no smoke asserts one. A stub `x-scheme-handler/http` handler logged only the `gio open` control probe and never the launcher's call. **Nothing prints any of this on the shipped path**, so a silent stderr is correct rather than a regression.
- **No smoke may trigger Preview in Browser.** `gio mime x-scheme-handler/http` answers `google-chrome.desktop` inside the devShell, because `XDG_DATA_DIRS` reaches the system applications directory even under the harness's `HOME` redirect, so on any host where the launcher works a smoke would open a tab in the developer's real browser. The server half is proved headlessly by `crates/wolf-core/tests/browser_preview.rs`.
- **`pane_layout::resolve_leaves`' refusal reaches no smoke.** Arranging a leaf whose Neovim window has left both snapshot maps while its pane survives is timing-dependent under a compositor. Its table is the whole of the coverage, and the same holds for the restore caps past [`restore_caps.rs`](#restore_capsrs), which need a hand-edited file rather than a session.
- **The restore's seed of the window stacking order has no smoke.** Under `cage` each window gains focus as it maps, which rebuilds the order without the seed, so no save tells the two apart. [`stacking.rs`](#stackingrs) covers the seed rule, and nothing covers its install in `restore_session`.
- **The restored window order on mutter.** The three-window restore smoke shows the present chain under `cage`; whether mutter focuses a restore's windows as they map is a person check.
- **The window's close button and a refused Quit's raise.** No smoke clicks the header bar's close button, which takes the same `close-request` check as Close Window, and `cage` declines `present()`, so whether a refused Quit raises the unsaved tab's window is a person check on a session compositor.
- **A quit during the restore's present chain.** No harness quits within a launch's first frames, so no test reaches `WindowHandle::cancel_present` with a present still pending.
- **The restore seed is table-tested only.** A restored tab contributes its seed to a commit made before its own first gather, which another tab's wake can make, and the next commit replaces it with the live gather, so no smoke's frozen save tells the two apart.
- **The glue that pairs a tab's cache with its page position.** `state_seam::ordered_tab_states` is table-tested, but nothing executes `WindowHandle::cached_window`'s pairing of each live cache with `TabHandle::page_position`. A glue bug pairing each cache with its `enumerate()` index instead would satisfy every headless row and persist insertion order again, and `three_tabs_survive_a_relaunch_under_cage` cannot discriminate it: in a window nobody has rearranged, position and index coincide. Closing it needs `a_dragged_tab_is_persisted_in_its_new_position_under_cage` and a capability no smoke has — dragging a bar entry under `cage` — so the next unit that touches the tab bar owes that one.
- **Only the drag reorder route is established.** `AdwTabBar` reorders by drag with no property that turns it off, and `AdwTabView:shortcuts` sits at a default that carries `Ctrl+Shift+Page_Up` / `Ctrl+Shift+Page_Down` — but `window::wire_keys`' bubble-phase controller maps `Page_Up` and `Page_Down` whatever the modifiers hold and answers `Propagation::Stop`, so the chord very likely reaches Neovim instead of `AdwTabView`. No test drives either route; the page-order rules rest on the drag, and nothing may claim the chord until someone measures it.
- **Two tab-lifecycle arms are unreachable from the shipped UI.** `TabsHandle::open_tab`'s `None` return needs a core that fails to start or a window already closing, and the close handler's dropped-`Weak` arm needs the tab layer gone while its view still emits. They are one message and one refused veto.
- **A tab's tooltip reaches no pixel.** No smoke hovers a tab, so the tooltip's escape and an empty tab's lack of one are [`tab_titles.rs`](#tab_titlesrs)'s alone.
- **Three things about a tab's title reach no pixel.** [`smoke_headless/tab_bar.rs`](#smoke_headlesstab_barrs) reads a directory's name, which `~/…` and an absolute path both contain, so the abbreviation is table-tested only. That a background `:cd` leaves the window title alone is unread, since the selected tab's entry draws the header's text. Nothing reads libadwaita's fade of a title too long for its tab.
- **A tab leaving the empty state is retitled in the same wake, and nothing asserts it.** Without the settle's `refresh_label` call the tab would lag its window's title only until its next wake, which no smoke can hold off.
- **The in-header tab bar's drags, its focus return and its build-time placement.** `cage` neither moves nor stacks windows, so no smoke drags a tab out of a window or in from another, reorders tabs inside the header, or drags the window by the header the tabs leave; those are person checks. No smoke puts focus inside the bar before it moves, so `TabBarSlot`'s focus return reaches no assertion. A window restored with two tabs or more places the bar in the header at build, and the restore smokes that do so read no header.
- **The sidebar's release of a closing tab's core.** No smoke closes the tab whose core the tree's search or a running project search is on, so `TreeHandle::release_core`'s owed restart and `SearchPanelHandle::release_core`'s positive branch run in no test: the native-tabs smoke closes the second tab, while the tree's search started on the first.
- **The crash overlay following a switch.** The crash smoke kills a background tab's Neovim and never selects it, so no test switches to a tab whose Neovim is dead or wedged.
- **No smoke checks what the window's activation and deactivation edges do in a real Neovim.** `notify::is-active` drives them, and under `cage` a window turns active as it maps and resigns only when another toplevel maps, such as a dialog or a later window, so [`tab_focus.rs`](#tab_focusrs) covers the rule alone; a person must switch away from the window and back after another tab saved a file.
- **A restored tab whose core fails to start, and New Tab over a dead or wedged selected tab.** No scenario arranges either, so that the window opens with its other tabs, and that New Tab is ungated, rest on the code alone.
- **Whether a page switch's `grab_focus` lands.** Measured inside `connect_selected_page_notify`: it succeeds on both the append path and the switch path, so no idle retry is armed. Nothing asserts it, the same shape as the palette's focus return above.
- **The native-tabs smoke's capture-B must-not-contain has no isolated red.** What it catches is two tabs' pane views drawn at once, reachable only by a structural edit that appends both to one visible container. Like the drawer strip's place after a rebuild, above, it turns on which widgets a container's children are, which only such an edit changes.
- **`the_shared_smoke_config_pins_the_appearance_every_ocr_smoke_assumes` has no falsifier in `src/` by construction.** It guards a constant in the test file, so both its red runs are edits to that file rather than to production code — a tripwire on the pin, not coverage of anything the application does.
- **`frame_metrics::flush_final`'s armed path runs in no test.** Every `cage` scenario's teardown signals the application, and the exit flush runs only after a clean return from `Application::run_with_args`, so the final interval's samples are discarded by design and nothing observes the flush at all. `maybe_emit`'s other branch goes the same way: an elapsed interval that recorded nothing is re-baselined without being summarized, and no assertion distinguishes that from a report that was never due.
- **Four rules of the armed glue have no observer.** That an unopenable `WOLF_FRAME_STATS_FILE` degrades to stderr-only rather than refusing to run; that `install()` resolves the path through `frame_stats::report_file_path` rather than filtering inline, pinned as a value function and unchecked on the glue side; that a `Scope`'s `Drop` records what an explicit `end` would; and that the `debug_assert!` in `with_stats` catches a record from another thread, which no call site can reach. [`frame_metrics_gate.rs`](#frame_metrics_gaters) reaches none of them: nothing in it may call `install()`, and its `Scope` drops against an absent recorder.
- **The multi-window merge of one accumulator.** N windows run N drain loops over one process-global accumulator, and whichever wakes first after the interval elapses emits the process-wide report. The frame-stats smoke launches one window, and nothing counts a report against the windows that contributed to it.
- **`wakes/key` is an upper bound no assertion can tighten.** It is a ratio of two counters, and mouse, scroll, resize and out-of-band core notifications all inflate it, so there is no value a smoke could assert it against — that smoke deliberately asserts none.
- **The readiness file's `layout-pending` condition has no end-to-end falsifier.** Its only falsifier is its unit row in `ready_seam.rs`. No smoke has been measured with the check skipped against a restore realized on each tab's own first wake, so whether any smoke ever waits on it is unknown.
- **Nothing observes a desktop shell using the installed icon or the desktop entry.** The packaged smokes run under `cage`, which has no shell, no launcher and only its own private session bus. What is shown is that the icons install at the sizes their directories name, that `desktop-file-validate` accepts the entry inside the derivation, and that `Exec` runs the packaged `wolf` with `%f` for Markdown files and folders. That a launcher draws the icon, associates a running window with the entry, or runs the entry for a file in GNOME Files, is unobserved.
- **The fill of a named background tab.** Every request here lands in the frontmost window, so no smoke sends `wolf D` from the terminal of an empty background tab; that the fill replaces that tab, not the selected one, rests on `open_folder`'s `current` and [`open_request.rs`](#open_requestrs)'s route.
- **`StartupWMClass` is tied to nothing.** `app.rs`'s `APP_ID` is a private const, and the desktop entry repeats its value as a literal no test reads — `packaged_layout.rs` retypes the same string for the `Icon` key and the icon filenames and never looks at the class. A renamed application id would break the window-to-entry association with nothing failing anywhere. Closing it needs the id exported from the crate, or an assertion against the built binary's own bytes.
- **The bundled-face half of the packaged fontconfig is unexercised.** `packaged_binary_renders_typed_text_under_cage` runs with `FONTCONFIG_FILE` scrubbed and reads glyphs back, but the face it renders is `DejaVu Sans Mono`, which the packaged fragment resolves through its `<include>` of the host configuration rather than through its own `<dir>`. `packaged_layout.rs` pins the installed files and the fragment's text; that a packaged launch resolves a bundled face through that directory is not shown.
- **The lighthouse pixel band has no taken red run.** The isolating edit — leave the tree installed and activatable and change what it paints in the installed copy, so `vim.g.colors_name` still reads `lighthouse` — is written down in the smoke and was not executed, and deleting the tree falsifies the readback four assertions earlier without reaching the band. The orlando band's equivalent recipe was taken, by turning `code_block` off in the installed copy, and reddens that band alone.
- **The `v:progpath` equality has no red run either.** A red run needs the package wrapper pointed at a Neovim build other than `wolfNeovim`, and none was taken. The assertion is live on real data — both launches read the same unwrapped store path — and is recorded as unfalsified rather than claimed.
- **What that equality does not say is that both platforms run one Neovim.** Linux takes Neovim from the flake's `nixpkgs`, which `nix flake update` moves, while Bazel stages its own pin, which [tracks the flake lock by hand](../../../bazel/nvim/README.md#bumping-the-pinned-neovim-version).
- **A terminal pane's pointer, clipboard and focus glue.** [`terminal_seams.rs`](#terminal_seamsrs) pins the rules, but no smoke presses a button or turns a wheel over a terminal pane, copies or pastes in one, or watches the window's activation, so the any-button gesture, the pinned session, the Copy and Paste targets and the `terminal_focus` reports reach no assertion. Nor does which pane `draw_pane` paints as focused, or its redraw when the window's activation changes: [`terminal_paint.rs`](#terminal_paintrs) passes `focused` to the canvas itself.
- **A terminal pane's persistence in the live widget.** No smoke persists a terminal pane and relaunches, so `PaneInner::leaf_contents`' `WinContent::Terminal` arm reaches no assertion, and deleting it fails nothing. The terminal-leaf rows in [`terminal_seams.rs`](#terminal_seamsrs) and wolf-core's [`rebuild_layout.rs`](../../wolf-core/tests/README.md#rebuild_layoutrs) cover the layers below it.
- **Damage in the live widget, and HiDPI.** [`terminal_paint.rs`](#terminal_paintrs) shows the canvas repaints only damaged rows, with no widget; the terminal-pane smoke reads final pixels, which a pane repainting every row on every wake paints too. Nothing runs at a device scale above 1.
- **The other-pane no-op in the live widget.** No smoke presses Preview or Edit in Another Pane with the drawer focused, so the shell's mapping of `PaneHandle::other_pane_target`'s `None` to no RPC reaches no assertion. Only the `None` row in [`other_pane_seam.rs`](#other_pane_seamrs) pins the rule beneath it.
- **A hidden preview across a zoom.** No smoke zooms with a Markdown preview hidden, so that its web view comes back without a reload and at its scroll position reaches no assertion.
- **The `DrawerOpen` unzoom and the restart reset.** Toggle Terminal unzooms before the drawer opens, so `apply_drawer_open`'s unzoom finds nothing zoomed, and no smoke restarts Neovim while zoomed or with a launch rebuild pending. K4 in [`pane_zoom.rs`](#pane_zoomrs) pins the rule; the calls in `apply_drawer_open` and `reset_after_restart` reach nothing, and only the relaunch smoke reaches `apply_layout_rebuild`'s.
- **A zoomed drawer.** No smoke zooms the drawer with Zoom Pane, so Toggle Terminal closing a zoomed drawer and unzooming, and the other-pane commands doing nothing from it, reach no assertion in the live widget. [`smoke_headless/extras.rs`](#smoke_headlessextrasrs) restores a drawer zoomed, saves it as `"drawer"` and unzooms it from the palette, and nothing more. The rules are pinned in [`pane_zoom.rs`](#pane_zoomrs): K1 zooms the drawer, K7 keeps the zoom on a Toggle Terminal from it, K5 unzooms at its close, and K8 makes the other-pane commands do nothing from it.
- **Parts of restoring the extras.** No falsifier covers readiness waiting for a shown pane's restored view, or a capture skipping a pane whose restored view or scroll still waits to apply: each orders a race, between a typed key or a capture and the view, that no smoke can reliably bring about. No test restores a view on a pane a zoom hides, which keeps it until the pane is shown and settled. No test restores a pane zoom together with an open drawer, a zoom that waits for the drawer's bind and then for its pane's focus.
- **Closing the zoomed pane, and Toggle Terminal focusing the drawer.** Neovim moves its current window off a closed zoomed pane, or onto the open drawer Toggle Terminal focuses, and `update_active_pane` reads either as a focus move, which unzooms. So the live widget cannot tell them from the `PanesChanged` unzoom in `detect_closes` or the `ToggleTerminal` one in `zoomed_after`: deleting that call turns no smoke red, and keeping the zoom on a `ToggleTerminal` leaves the zoom smoke's H green. The call still matters for a zoomed pane that is not current, in an unselected tab. K5 and K7 in [`pane_zoom.rs`](#pane_zoomrs) pin the rules.
- **A split while zoomed.** `<C-w>v` makes the new window Neovim's current one, and `update_active_pane` reads that as a focus move, which unzooms. So the main zoom smoke's S leg cannot tell it from the `Split` unzoom in `apply_split`, and deleting that call leaves S green. O pins the rule live, since Preview in Another Pane leaves the current window on its source, and K4 in [`pane_zoom.rs`](#pane_zoomrs) pins it as a rule.

## Deliberate test gaps

Behavior this front-end has that is deliberately not smoked, with what covers it instead.

- **The primary menu popover.** The keystroke injector cannot click a header-bar menu button, and keyboard-navigating to it is too fragile to be worth the flake. The action namespace's own tests cover every item referencing a declared, implemented, non-reserved action and every label matching the command registry. Ten of the menu's items have their action activated end to end by the accelerator smokes, and the smokes that type `:WolfCloseWindow` perform Close Window's action through the command drain rather than through the menu's own action. The two-window smoke reaches New Window's action through the palette and Quit's through Ctrl+Shift+Q, so the two `app.`-scoped rows are exercised by a route of their own. No smoke reaches `win.close-tab`, though the palette runs its `UiAction`.
- **The About dialog.** Its widget layer is covered by nothing, and cannot be: the row is reachable only from the primary menu popover, itself a gap above. What covers it instead is where the weight is — the dialog's text is asserted from the compiled artifact, against every bundled font's licence phrase and, in [`about_notices.rs`](#about_noticesrs), the seven libraries', all re-typed locally rather than imported, so the assertion is about what ships; the row's status is pinned by `actions.rs`, and its `UiAction` by `command_action.rs`.
- **The header-bar buttons' click path**: no pointer smoke clicks Back or Forward. A header-bar button offers no probe click for Neovim to report back, so the honest alternatives are a theme-owned inset pinned as a second measured constant with nothing to cross-check it, or distorting the buttons to buy hit margin. What covers them instead: the two chord smokes drive the same `gio` actions end to end, and [`nav_ui.rs`](../src/README.md#nav_uirs)'s tables cover the sensitivity and per-pane rules.

  [`smoke_headless/sidebar_toggles.rs`](#smoke_headlesssidebar_togglesrs) does click the sidebar's toggle group from such an inset, because the group's state shows in its own pixels and its effect in the sidebar band, so a wrong target fails instead of passing.

- **The toggle group's insensitivity in the empty state.** No smoke clicks the group in a project-less window. A Search click there would be refused by `search_panel::show_decision` either way, and a Files click would leave the sidebar hidden, but would set the preference a later folder shows; only `apply_selected_tab` desensitizing the group stops that, and nothing asserts it.
- **The Open File and Open Folder dialogs.** Neither is driven, since no portal backend exists under `cage`: their callback bodies are trivial and the empty state's presentation is smoked. What Open Folder does with the folder is not a gap: an Open Recent palette row reaches the same `AppShell::open_folder`, which [`smoke_headless/open_folder.rs`](#smoke_headlessopen_folderrs) and `open_recent_from_a_folder_tab_opens_a_fresh_window_under_cage` drive through every step. The save dialog is driven, under the in-process fallback [Smoke conventions](#smoke-conventions) requires.
- **The search panel's other routes**: a result activated over a preview pane, a mouse click on a row, and a file row's collapse toggle. `activation_route`'s table covers the preview route, and `row_activation` and `panel_rows` cover the toggle. No smoke clicks a result row.
- **Unsaved-edit recovery as a separate scenario.** `crates/wolf-core/tests/swap_recovery.rs` covers the mechanism headlessly and deterministically. Rather than duplicate it with a flakier compositor version, the crash smoke — which already makes an unsaved edit and kills Neovim — asserts the marker on a pixel after recovery and its absence in the pre-crash control, which adds the one thing a core test structurally cannot have: the front-end driving the reopen itself.

## The `key_from_char` helper

`gdk_key.rs`, `roundtrip_nvim.rs`, and `actions.rs`'s own tests each define a small `key_from_char(c: char) -> gdk::Key` helper, because gdk4-rs has no `Key::from_unicode`. It builds the keyval exactly as GDK would from a Unicode scalar:

```rust
use gdk::glib::translate::from_glib;
fn key_from_char(c: char) -> gdk::Key { unsafe { from_glib(gdk::unicode_to_keyval(c as u32)) } }
```
