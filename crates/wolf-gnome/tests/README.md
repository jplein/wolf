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

- Pure: no display, Neovim or filesystem; `is_dir` answers from a fixed list. Whether the coordinator feeds them the live windows is [`smoke_headless/restore.rs`](smoke_headless/README.md#restorers)'s question.

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
- Whether the paned reads and writes the share is [`smoke_headless/drawer_divider.rs`](smoke_headless/README.md#drawer_dividerrs)'s question.

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

- Pure: no display, no Neovim and no WebKit. Whether the pane view captures at the right moments and the page posts and applies a scroll is [`smoke_headless/extras.rs`](smoke_headless/README.md#extrasrs)'s question.

#### `pane_zoom.rs`

**Covers**

- `pane_zoom`'s rules, K1 to K5 and K7 to K14, with the ids of [`PaneZoomTests.swift`](../../wolf-macos/Tests/WolfKeyMapTests/README.md#panezoomtestsswift). K6 is absent, since it tests the frozen dividers and no divider exists while a view here is zoomed, and K10 tests `shown_alone` over bound ids where the Swift tests `visibleRects`. K11 to K13 compose `focus_origin`, `drawer::focus_target` and `zoomed_after` as `apply_focus` does, over a plus-shaped tree above the drawer.
- The saved zoom through `pane_layout`: G6a pins `leaf_index`, the depth-first index a saved tab's `zoomed` names, and the drawer's `None`. P3 is this file's own, the `leaf_index`, `encode` and `decode` round trip a relaunch makes, since this front-end has no FFI push-builder for the Swift P3 to cover.
- `restore_targets` (E10): a saved zoom and focus mapped onto the rebuilt leaves, the drawer only while it restores open, and a zoom winning the focus.
- `ZoomEvent::OpenFile` (Z1) unzooming whichever pane is zoomed, the drawer included.

**Gotchas**

- Pure: no display, no Neovim and no compositor. What the pane area shows for an answer is [`smoke_headless/zoom_pane.rs`](smoke_headless/README.md#zoom_paners)'s question.
- A case for a rule both front-ends apply belongs on both, or the two ports can drift silently.

#### `preview_root_style.rs`

**Covers**

- `preview_width`'s rules. A capped `root_style`'s content box lands on `editor_inset::grid_rect`'s capped rect in a pane wider and in one narrower than the cap, its width exactly and its left edge within the floor `grid_rect` takes; a full-width cap pads without capping.
- `styled_document` over a real `render_html(…, WidthMode::Full)` puts the style on the `<html>` tag alone and leaves the document byte-identical from `<head>` on, or returns a document it cannot style unchanged; `apply_root_style_js` decodes to exactly the style.

**Gotchas**

- Pure: no display, no Neovim and no compositor. The content box comes from `content_box`, a model of CSS block layout for a `border-box` root; whether WebKitGTK lays the page out so is [`smoke_headless/preview_margins.rs`](smoke_headless/README.md#preview_marginsrs)'s question.

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
- What the widgets do with these answers is the search smokes' in [`smoke_headless/sidebar.rs`](smoke_headless/README.md#sidebarrs) and [`smoke_headless/search_toggles.rs`](smoke_headless/README.md#search_togglesrs).

#### `sidebar_toggles.rs`

**Covers**

- The pure half of the header bar's toggle group: `active_toggle` for each shown flag and page, `toggle_click` for every combination of shown, page and clicked, and `toggle_at` either side of the group's middle, left to right and mirrored right to left.

**Gotchas**

- Pure: no display, no Neovim and no compositor. Every expected value is written out by hand.
- Whether the window wires the answers up is [`smoke_headless/sidebar_toggles.rs`](smoke_headless/README.md#sidebar_togglesrs)'s question.

#### `smoke_headless/`

The `smoke_headless` test binary: the smokes that launch the built app under `cage`, their shared harness and the compositor-free guards over them, and the conventions a smoke keeps. Its files are described in [`smoke_headless/README.md`](smoke_headless/README.md).

#### `stacking.rs`

**Covers**

- `StackingOrder`: a focused window moving to the front, a minimized one behind every window that is not, focus unmarking it, the seed keeping the saved order and minimized marks, and unranked windows following the ranked ones in registry order. `TabRecency`: the seed's active tab first, and a selected tab becoming the most recent.

**Gotchas**

- Pure: no display, no Neovim and no compositor. Whether the `is-active` handler feeds `focus` is [`smoke_headless/restore.rs`](smoke_headless/README.md#restorers)'s two-window smoke's question.

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

- Pure: no display, no Neovim and no compositor. Whether the window applies the answer, and applies it again as tabs open and close, is [`smoke_headless/titlebar_tabs.rs`](smoke_headless/README.md#titlebar_tabsrs)'s question.

#### `tab_close.rs`

**Covers**

- `tabs_ui::close_page_now`'s table: a live tab is asked once and only `NoAnswer` closes it, while a dead, stalled or gone core and a confirmed close close without asking.
- The tables of `tabs_ui::tab_unsaved`, `window_close` and `quit`: a confirmed tab is `Clean` and a core not `Alive` is `Unreachable`, both unasked, and `NoAnswer` is `Unreachable`. The first `Unsaved` is shown, windows in the order given, and nothing after it is pulled; `Unreachable` beside `Clean` closes everything.
- The `win.close-tab` row: `Command::CloseTab`, no accelerator or fold, `MenuOnly`, `action_for_command`'s answer, and the menu item directly before Close Window.

**Gotchas**

- Pure: no display, no Neovim and no compositor. Whether `on_close_page` passes the page's own confirmation, liveness and ask, and selects a background page first, is [`smoke_headless/close_tab.rs`](smoke_headless/README.md#close_tabrs)'s question. Whether the close handler and `Coordinator::quit` ask and show the tab is [`smoke_headless/close_window.rs`](smoke_headless/README.md#close_windowrs)'s.

#### `tab_focus.rs`

**Covers**

- The tab focus trigger's rule: `tabs_ui::focused_tab` answers the selected tab while the window is active and none while it is not, and `tabs_ui::focus_edges` sends nothing for an unchanged focus, the loss before the gain on a switch, and one edge on activation or deactivation.

**Gotchas**

- Pure: no display, no Neovim and no compositor. Whether the edges reach the right tab's core is the reload smoke's question, in [`smoke_headless/panes.rs`](smoke_headless/README.md#panesrs).
- The activation and deactivation rows are this file's alone. Under `cage` a window turns active as it maps and resigns only when another toplevel maps, such as a dialog or a later window, and no smoke checks what either edge does.

#### `tab_titles.rs`

**Covers**

- `tabs_ui::tab_label`'s rules, TL1 to TL6: a folder tab is titled `~/…` under home and by its absolute path elsewhere, with the absolute path as its tooltip, Pango-escaped there and not in the title. An empty tab, or a folder tab whose Neovim has reported no directory, is titled `WINDOW_TITLE_FALLBACK` with no tooltip.

**Gotchas**

- Pure: no display, no Neovim and no compositor. Whether `TabHandle::refresh_label` passes the tab's emptiness is [`smoke_headless/new_tab.rs`](smoke_headless/README.md#new_tabrs)'s question, and whether each tab follows its own Neovim live is [`smoke_headless/tab_bar.rs`](smoke_headless/README.md#tab_barrs)'s.

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

- Pure: no display, no Neovim and no compositor. Whether the window paints the answer is [`smoke_headless/titlebar.rs`](smoke_headless/README.md#titlebarrs)'s question.

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

## What is still unproven

A measured gap, something the code does that no test observes, is recorded in [What is still unproven](../../../notes/crates/wolf-gnome/tests/what-is-still-unproven.md) under `notes/`. Read it before ticking a behaviour as covered, and add to it when a smoke is found not to discriminate.

## Deliberate test gaps

Behavior this front-end has that is deliberately not smoked, with what covers it instead.

- **The primary menu popover.** The keystroke injector cannot click a header-bar menu button, and keyboard-navigating to it is too fragile to be worth the flake. The action namespace's own tests cover every item referencing a declared, implemented, non-reserved action and every label matching the command registry. Ten of the menu's items have their action activated end to end by the accelerator smokes, and the smokes that type `:WolfCloseWindow` perform Close Window's action through the command drain rather than through the menu's own action. The two-window smoke reaches New Window's action through the palette and Quit's through Ctrl+Shift+Q, so the two `app.`-scoped rows are exercised by a route of their own. No smoke reaches `win.close-tab`, though the palette runs its `UiAction`.
- **The About dialog.** Its widget layer is covered by nothing, and cannot be: the row is reachable only from the primary menu popover, itself a gap above. What covers it instead is where the weight is — the dialog's text is asserted from the compiled artifact, against every bundled font's licence phrase and, in [`about_notices.rs`](#about_noticesrs), the seven libraries', all re-typed locally rather than imported, so the assertion is about what ships; the row's status is pinned by `actions.rs`, and its `UiAction` by `command_action.rs`.
- **The header-bar buttons' click path**: no pointer smoke clicks Back or Forward. A header-bar button offers no probe click for Neovim to report back, so the honest alternatives are a theme-owned inset pinned as a second measured constant with nothing to cross-check it, or distorting the buttons to buy hit margin. What covers them instead: the two chord smokes drive the same `gio` actions end to end, and [`nav_ui.rs`](../src/README.md#nav_uirs)'s tables cover the sensitivity and per-pane rules.

  [`smoke_headless/sidebar_toggles.rs`](smoke_headless/README.md#sidebar_togglesrs) does click the sidebar's toggle group from such an inset, because the group's state shows in its own pixels and its effect in the sidebar band, so a wrong target fails instead of passing.

- **The toggle group's insensitivity in the empty state.** No smoke clicks the group in a project-less window. A Search click there would be refused by `search_panel::show_decision` either way, and a Files click would leave the sidebar hidden, but would set the preference a later folder shows; only `apply_selected_tab` desensitizing the group stops that, and nothing asserts it.
- **The Open File and Open Folder dialogs.** Neither is driven, since no portal backend exists under `cage`: their callback bodies are trivial and the empty state's presentation is smoked. What Open Folder does with the folder is not a gap: an Open Recent palette row reaches the same `AppShell::open_folder`, which [`smoke_headless/open_folder.rs`](smoke_headless/README.md#open_folderrs) and `open_recent_from_a_folder_tab_opens_a_fresh_window_under_cage` drive through every step. The save dialog is driven, under the in-process fallback [Smoke conventions](smoke_headless/README.md#smoke-conventions) requires.
- **The search panel's other routes**: a result activated over a preview pane, a mouse click on a row, and a file row's collapse toggle. `activation_route`'s table covers the preview route, and `row_activation` and `panel_rows` cover the toggle. No smoke clicks a result row.
- **Unsaved-edit recovery as a separate scenario.** `crates/wolf-core/tests/swap_recovery.rs` covers the mechanism headlessly and deterministically. Rather than duplicate it with a flakier compositor version, the crash smoke — which already makes an unsaved edit and kills Neovim — asserts the marker on a pixel after recovery and its absence in the pre-crash control, which adds the one thing a core test structurally cannot have: the front-end driving the reopen itself.

## The `key_from_char` helper

`gdk_key.rs`, `roundtrip_nvim.rs`, and `actions.rs`'s own tests each define a small `key_from_char(c: char) -> gdk::Key` helper, because gdk4-rs has no `Key::from_unicode`. It builds the keyval exactly as GDK would from a Unicode scalar:

```rust
use gdk::glib::translate::from_glib;
fn key_from_char(c: char) -> gdk::Key { unsafe { from_glib(gdk::unicode_to_keyval(c as u32)) } }
```
