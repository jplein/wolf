# wolf-gnome/tests

Integration tests for [`wolf-gnome`](../README.md). This crate is Cargo-only by design — it has no Bazel target, so there is no `rust_test` wiring to keep in sync, and `cargo test` discovers every file here.

```sh
nix develop --command cargo test -p wolf-gnome
```

The devShell is not optional: it supplies GTK, exports `WOLF_NVIM_BIN` and `WOLF_RG_BIN`, and pins the font stack through a generated `FONTCONFIG_FILE`, which is what makes Pango measurement and Cairo rasterization reproducible enough to assert on pixels. The compositor smokes below run in that same default invocation and are the slowest part of it.

Rules that are decidable from their arguments are pinned in each seam's own `#[cfg(test)] mod tests` rather than here, which is the point of the seam split in [DECISIONS.md](../DECISIONS.md). The files below cover what a seam cannot reach: a real Neovim, and a real window.

## Files

#### `gdk_key.rs`

**Covers**

- Table-driven `gdk_key(keyval, state) -> Option<KeyEvent>`: named keys, arrows, F-keys, the printable shift contract (`shift` is always false for `Key::Char`; the keyval carries the case), modifiers on both named and printable keys, and bare modifier/lock keys yielding `None`. Runs with no display and no nvim — `gdk::Key` constants are compile-time and printables are built with the local `key_from_char` helper, since gdk4-rs has no `Key::from_unicode`.

**Gotchas**

- Assert only on letter casing and named keys. Shifted-punctuation unicode (that Shift+`,` yields `<`) is keyboard-layout dependent and would be flaky.

#### `mouse_nvim.rs`

**Covers**

- The pure pointer seams driven into a real `WolfCore` and a real Neovim, doing by hand what a gesture or scroll callback would do: `cell_at` for the cell, `mouse_input` for the button and modifiers, the accumulator and `scroll_steps` for the deltas, then `send_mouse` and `send_scroll`. A press, drag, and release select a range; the drag point is asserted to clamp to the last cell before it is sent. A sub-cell scroll carry crosses exactly once, a positive delta strictly increases the viewport's top line, and a negative one strictly decreases it.
- The refusal of a non-primary button, so no context-menu click can reach Neovim.

**Gotchas**

- No GTK controller is involved, so this file cannot say whether a controller ever fires. That is the smokes' question, and the two deliberately do not overlap.
- Cell sizes are locally declared rather than measured, because the seams are pure arithmetic and need no font.
- The buffer fixture's lines must stay five characters wide, or a click at column 2 clamps to end-of-line and the exact-column assertion means nothing. A literal double quote cannot appear inside a `:put =` expression, which is why the format string is quoted with doubled single quotes.

#### `paint_golden.rs`

**Covers**

- `paint` onto an offscreen ARGB32 `ImageSurface` from a hand-built 4x2 `Snapshot`: exact equality on known background fills, background-only interiors for a wide-trailing empty-text cell (the skip-glyph path), and at least one non-background pixel in a glyph cell. Needs no display and no nvim, but does need the pinned DejaVu Sans Mono via `FONTCONFIG_FILE`.

**Gotchas**

- Deliberately not a whole-image hash, which is too brittle across freetype/fontconfig versions. Pixel reads assume little-endian ARGB32, in-memory byte order `B, G, R, A`.
- The PNG golden is a dev aid nothing asserts against; regenerate it with `nix develop --command cargo test -p wolf-gnome --test paint_golden -- --ignored regenerate_png_golden`, which needs the `png` feature on the `cairo` dependency. It is the one ignored test in the repo.

#### `roundtrip_nvim.rs`

**Covers**

- The full input seam against a real Neovim: `gdk_key` to `InputEvent::Key` to `WolfCore` to snapshot. It types `Hello` in insert mode, feeding the leading `H` as the already-uppercased keyval plus the `SHIFT` modifier mask exactly as GDK reports it, and asserts row 0 contains the word — the shift-glyph contract end to end.

**Gotchas**

- Requires `WOLF_NVIM_BIN`; `common::require_nvim_bin` panics rather than skipping. No display needed.
- Leaving insert mode emits several follow-up flushes, so the test drains and sleeps a settle window before reading the snapshot. A new case that reads a snapshot needs the same treatment.

#### `smoke_headless.rs`

**Covers**

- Thirty-three end-to-end smokes that launch the built binary under `cage`, drive it with `wtype` and [`wolf-pointer-inject`](../../../tools/wolf-pointer-inject/README.md), screenshot the live window with `grim`, and read it back with `tesseract` in sparse-text mode, plus a second uniform-block reading in the divider-relaunch smoke, which is red without it.
- The scenarios: text rendering, sidebar population, two picker cases, two command-palette cases (one an Open Recent row), two buffer-picker cases (a typed query, and a bare Enter on the preselected row), one project-search case, five split-pane cases, and five command-drain cases.
- The rest: four pointer cases (including the drag clamp and the click-then-close focus regression), two divider-drag cases, two crash-recovery cases, one stall case (`SIGSTOP` the captive Neovim, then let the buffer picker's blocking call elapse), three application-state cases across a relaunch, one navigation case (`Alt+Left`), and one sidebar-toggle case (`Ctrl+Shift+B`).
- Four compositor-free guards on the smokes' own fixtures and harness, which need no toolchain and run in microseconds. The target therefore reports `37 passed`, not `33`.

**Gotchas**

- These are not `#[ignore]`d. Run only them with `nix develop --command cargo test -p wolf-gnome --test smoke_headless`.
- A new scenario must park the block cursor off the asserted row (a normal-mode `o` then Escape), since the inverted cursor block corrupts the cell it covers, and must keep sparse-text page segmentation, since the default mode returns nothing on the light-on-dark editor grid.
- Read [Smoke conventions](#smoke-conventions) before adding one. Several of the rules there exist because a smoke passed while proving nothing.

#### `common/mod.rs`

**Covers**

- Shared helpers for this crate's integration tests: `require_nvim_bin`, `init_lua_path`, `fresh_cwd`, `config`/`config_in`, and `wake_channel` returning a `WakeFn` paired with a `FlushWaiter` (`wait_for_flush`, `drain`).

**Gotchas**

- These duplicate `wolf-core`'s equivalents because Cargo cannot share a `tests/common` module across crates; they are kept in sync by hand.
- `fresh_cwd` directories are not auto-deleted — the cwd is nvim's working dir and file-search root, so it must outlive the session.

#### `../testdata/fonts.conf`

Template fontconfig that exposes only the pinned DejaVu font directory and deliberately omits every host font dir, so Pango measurement and Cairo rasterization are reproducible across machines. The flake's `shellHook` substitutes `@DEJAVU_FONTS@` with the Nix store path of `pkgs.dejavu_fonts`, writes the result to a temp file, and points `FONTCONFIG_FILE` at that copy — so this file is never consumed directly. Edit it here; the generated copy is rebuilt on each `nix develop`.

#### `../testdata/paint_golden.png`

A write-only render of `paint_golden.rs`'s fixture snapshot, kept for eyeballing a renderer change. No test reads it, so it can never fail the suite and does not have to be updated when `paint` changes; refresh it with the `regenerate_png_golden` command above when a visual diff is useful.

## Smoke conventions

The serialization, the single-shell-command shape, and the single environment helper are in [DECISIONS.md](../DECISIONS.md). What follows is what a smoke author has to get right.

**A fixture must not be named after the term its smoke asserts on.** The sidebar renders every file in the working directory by name and nvim's `:e` echoes the filename, so a fixture carrying the asserted word satisfies the assertion whether or not the feature did anything — a permanent silent pass, not a flake. Fixture names and asserted terms are therefore deliberately unlike: a heading lives only in a file's content. The one exception is the sidebar-population smoke, where the rendered row is the feature under test.

That rule has three mechanical guards, because the fixture and the assertion it can disarm sit a thousand lines apart. One asserts that no sidebar filler filename contains any asserted term; one asserts the same of the temp working directory's two fixed path components, which the header bar renders as page text; and one asserts it of the relaunch fixtures.

The third guard carries more than names. It also pins the two fixtures built from a body rather than a bare heading — the project-search target down to the match's line index and its byte offset, since that smoke's line and column assertions are derived from them — and two crossed pairs, the title smoke's project and home directories and the project-search smoke's target and starting headings, in which neither member may carry the other's term.

The tables of asserted terms are built from the assertion constants themselves rather than retyped, so they cannot drift, and every term is checked to be lowercase, since the OCR output is lowercased before comparison.

**A red-run demonstration is a deliverable, not a formality.** A check can be aimed at the right property and still look somewhere the defect cannot appear: a press-and-release at two different cells enters visual mode by itself, so a drag assertion needs a round trip whose press and release are at the same cell to discriminate drag events at all; a wrapper that refuses to exec is never named `nvim`, so a child-name scan cannot see an uncapped retry loop; and a centered button spans the same midpoint whatever width it requests, so a center-derived click point is blind to a width regression. Each of those reads as correct and stays green against a deliberately broken build; only running the revert exposes it.

**Exit codes carry meaning, and folding two causes into one code destroys evidence.** A process that vanishes because the main loop panicked looks exactly like one that shut down cleanly, so "exited non-zero" gets its own code rather than joining "still alive"; likewise "the app died with its Neovim", "not exactly one live child", and "the killed child never left the process table" are each distinct.

**Every smoke launches in a fresh temp working directory, and that is mandatory.** The captive Neovim keeps a durable swap directory keyed on the cwd, which deliberately outlives the process so a fresh nvim can recover a crashed one's unsaved edits. At a fixed cwd, one run's leftover swap file makes the next run's startup buffer raise a blocking prompt that eats the first keystroke, and the smoke goes red for whatever change happens to be in flight.

**The rendered page must be identical on every run, not merely dense.** The temp cwd is a unique parent holding a fixed-name child, and the helper hands the parent to the app as its home directory, so the title seam abbreviates the varying part away and every header bar renders the same string. A run-varying title makes an unrelated, correct smoke stochastic; more page text helps only when it is the same text every time. Sparse pages have the opposite failure: an isolated leading glyph is misread as a ligature, which is why several smokes drop filler files purely for density. If that regresses, add filler files rather than weakening an assertion.

**No pointer smoke hardcodes a pixel.** The cell size is measured through the same metrics path and font size the app is launched with, one probe click establishes the origin, and every later target is expressed as a delta from it. The derived pane origin is checked as a loud, self-checking precondition whose failure message names the candidate causes in order.

A divider handle cannot be modelled at all. Its width is theme-owned and its position is a fraction of the paned's `max_position`, so a drag smoke locates it by bisecting clicks against the window Neovim reports as current, with a reset click at the low seed before each probe — a click that lands on the handle changes no window, so without the reset that outcome reads as whichever pane the previous probe focused, and the search converges on the wrong side. The seeds the Rust side derives are seeds only.

**Under `ext_multigrid`, a width readback must use `winwidth()`.** Resizing a grid changes the window's grid width, which `winwidth()` reports, and leaves its layout width, which `nvim_win_get_width()` reports, untouched. A readback built on the latter is blind to a divider drag — it reports 22 and 22 cells either side of a drag a screenshot shows plainly — and it drifts on its own across a relaunch.

**Where the smokes' durable state goes is asserted, not assumed.** The environment helper redirects the state directory, and the relaunch smoke checks in four parts that the redirect held: that the test process has no such variable of its own, that the smoke's own directory holds a state file, that neither home-derived fallback directory does, and that the developer's real state file is byte-identical across the run — including the normal case where it does not exist and must still not exist afterwards.

## Pointer injection

Real pointer events reach the GTK controllers through [`wolf-pointer-inject`](../../../tools/wolf-pointer-inject/README.md), this repo's own long-lived virtual-pointer client, built by `flake.nix`. Its README owns the reasons it must be long-lived, why the off-the-shelf tools cannot work, and its command grammar; do not restate them here.

Seven smokes use it — the four pointer scenarios, the two divider drags, and the crash-overlay one, which needs a real click on the restart button. Each starts the helper inside `cage` before the app, so the seat already carries pointer capability when GTK binds, waits for its readiness line, and then drives it over a FIFO with a poll-for-ack protocol: an ack means the compositor has processed the event, not that the bytes left a buffer.

The scenarios hold the FIFO's write end open on a dedicated descriptor for their whole lifetime, since each write would otherwise close it and hand the helper a spurious end of file, and a trapped cleanup kills the helper before the app so shutdown never races an event.

**In-gesture commands are paced, and the pacing is load-bearing.** Neovim's mouse API transports only the button code through the typeahead while the position rides on globals the handler overwrites, so three calls landing a fraction of a millisecond apart all resolve at the last position. Pacing also stops GDK's motion compression folding intermediate waypoints into one drag update. If a drag smoke flakes, raise the pacing interval; never touch an assertion.

One diagnostic trap is worth writing down: libwayland's client-side debug output prints `interface#id.event` with a `#`, not `@`, so a grep for the wrong separator matches nothing and reads exactly like "no events were delivered".

## What is still unproven

Being scrupulous about this is the point of the section.

- **The strict press ordering.** That a click focuses the window is proven, because closing the clicked pane closes the right one. That the focus change is sent before the press is not observable, since Neovim's own left-mouse handling also moves the current window. It is guaranteed by construction — both ride one ordered channel — and must not be ticked.
- **The recorded pointer position.** The scroll handler names its target grid explicitly, so a stuck recorded position would still scroll the right window and the smoke would still pass. Nothing asserts on what the motion controller produces.
- **The press-pinned grid and button.** No smoke drags from one pane into another, so there is no other grid for a wandering drag to be retargeted to and the pin is never discriminating. Proving it needs a split, a press in one pane, and a release in the other.
- **The palette's focus return.** No test discriminates it: deleting `PaletteInner::close`'s `area.grab_focus()` leaves the command-palette smoke green across three runs, so that smoke's marker assertion is a precondition rather than a discriminator. Why hiding an overlay behaves differently from destroying a focus widget on a pane rebuild is unestablished — only the two outcomes are measured, and the pane-rebuild case reddens `click_then_close_keeps_keyboard_input_alive_under_cage` under revert. Treat the focus rule in [DECISIONS.md](../DECISIONS.md) as binding for every overlay until someone measures the difference.
- **The palette's arrow navigation and its Escape.** No smoke moves the palette's selection or closes it with Escape. The smoke's query is chosen to yield exactly one row, which is what makes its ranking deterministic. The arithmetic beneath both is the same `next_selection` / `restored_index` table the picker's is, and `PaletteInner::refresh`'s reset-vs-preserve rule under a changing query is a widget closure the fast suite cannot reach either.
- **`open_recent_candidates`' three refusals reach no pixel.** They are table-tested only; no smoke arranges a deleted, relative or window's-own-cwd candidate, because the palette closes before `grim` fires and a two-row query would make which row Enter activates depend on ranking tie-breaks.
- **`open_recent_row_is_matched_by_its_visible_label` has no independent revert.** The rank/display agreement is the absence of a post-rank rewrite, so there is no production line to revert; it reddens under the `rank_palette(&[], &[], …)` revert, and what it guards on its own is a later label rewrite.
- **The Open Recent MRU key shape has no falsifier in this crate.** `wolf_core::rank_palette` builds it, so both the smoke's readback and the seam's assertion on it are a join between two crates rather than a discriminator of anything here.
- **The ellipsized palette row label never reaches a pixel**, for the same reason `search_status` does not: `grim` runs once the overlay has closed.
- **`open_recent_candidates_table` asserts inside a `for` loop.** Measured: a revert breaking two of its rows produces one observed failure. Both rows are present and correct; only the evidence from a single red run is partial.
- **`PaletteInner::open`'s no-core path.** Empty candidates, with the palette still opening on the registry, is widget-bound and unreachable from the fast suite.
- **The buffer picker's arrow navigation and its Escape.** No smoke moves its selection or closes it with Escape. Its typed-query smoke is written to yield exactly one row, and the arithmetic beneath both is the same `next_selection` / `restored_index` table the file picker's is. The row-1 preselection is a separate matter and is not a gap: the bare-Enter smoke exercises it end to end.
- **The buffer picker's modified marker never reaches a pixel.** `MODIFIED_SUFFIX` is table-tested in the seam, but no smoke opens the picker with a modified buffer, and the marker glyph reads poorly through OCR in any case.
- **`BufferPickerInner::open`'s ordering.** That a refusal returns before any widget is touched, and that nothing is shown until the candidate snapshot returns, is widget-layer and invisible to both suites. It is written down in [`src/README.md`](../src/README.md) so nobody tidies the show-after-fetch ordering back.
- **The search overlay's arrow navigation and its Escape.** No smoke moves its selection or closes it with Escape; its query is chosen to yield exactly one row. The arithmetic beneath both is the same `next_selection` / `restored_index` table the other three overlays share.
- **The search status label never reaches a pixel.** `search_status` is table-tested in the seam, but the capture is taken after the overlay has closed, so no smoke reads the label — including its `WOLF_RG_BIN unset` arm. Whether a `SEARCH_CHAR_BUDGET`-length row label fits `SEARCH_BOX_WIDTH`, and whether the Pango ellipsize engages, are unobserved for the same reason: `grim` runs once the overlay is gone.
- **The in-flight cancellation on a second Enter.** Nothing observes a killed `rg` child.
- **`search_overlay::open_decision`'s refusals.** A refusal returns before any widget is touched, so it is invisible to both suites, including the empty-state arm. The buffer picker's identical entry above is the precedent.
- **`palette::every_registered_command_is_classified_by_the_palette` has no red run of its own here.** Measured: it stayed green under the revert that restores `Command::SearchInProject` to a `Deferred` classification.
- **The project-search smoke's assertion 6 has no isolated red run.** Measured: the line revert — sending `InputEvent::Open` where `open_at` belongs — aborts at assertion 4, so assertions 5, 6 and 7 are never evaluated under it, which is why a separate column-only revert exists. Measured: both reverts commissioned as assertion 6's own discriminator go red at the exit-status assertion with exit 14 instead. Assertion 7 has none by design, the same shape as the bare-Enter smoke's must-not-contain assertion.
- **`app::wire_keys`' buffer-picker guard.** Deleting `|| buffers.is_open()` leaves both buffer-picker smokes green across a run. The clause stays, and the file picker's, the palette's and the project search's clauses in the same guard are equally undiscriminated. No mechanism is offered for that measurement, because none was measured.
- **`apply_overlay`'s `buffers.close()` has no falsifier.** The buffer-picker box sits below the opaque stall overlay in the stacking order, so an unclosed one is invisible to the capture and the stall smoke's assertions do not depend on it. What the call prevents is an overlay left flagged open, which keeps `app::wire_keys`' guard suppressing every key to Neovim, including after a later restart; nothing smokes that.
- **The bare-Enter smoke's must-not-contain assertion has no isolated red run.** No revert can put two buffers' headings on screen in a single pane, so the assertion is reached only once an earlier one has already failed. What it contributes is naming which wrong row was activated, not detecting that one was — the same shape as the crash assertions below.
- **The `Ctrl+e → None` accelerator row is covered collectively rather than individually.** Any revert that breaks the shift-exactness of the chord matcher breaks it for all seven bound letters at once, and the table asserts in one sequential loop, so an earlier row fails first. The invariant has two independent red runs; this row has none of its own.
- **The one-overlay-at-a-time rule.** No smoke opens one of the four transient overlays while another is up. It is three idempotent `close()` calls per arm in `shell::perform`.
- **The stall overlay's Restart button on a wedged core.** The stall smoke captures the overlay but clicks nothing, and the crash smoke's click is against a dead core — so the one route out of a stall is unexercised end to end.
- **The `NotResponding` to `Alive` transition.** `wolf-gnome` cannot produce it at all, so this is unreachable rather than merely unproven: both blocking calls a running window issues refuse to run in that state. `crates/wolf-core/tests/nvim_liveness.rs` covers the mechanism against a genuinely wedged Neovim.
- **Restoring the window size.** `cage` forces the toplevel to the output size, so a restored size never takes visible effect and no screenshot can show one. The seam is table-tested and the window builder demonstrably reads it. Window position is a different matter and is not a gap: it is deliberately never persisted, per [DECISIONS.md](../DECISIONS.md).
- **A drag on a horizontal (stacked) divider.** Both drag smokes drag a vertical one. `set_divider_fraction` never reads a split's orientation, so what a stacked drag would add is the widget side: the orientation mapping into a vertical `gtk::Paned` and that paned's own travel.
- **The allocation guard on `notify::position`.** Nothing can resize the toplevel under `cage`, so the `max != last_max` arm — the one that tells a clamp GTK performed mid-allocation apart from a user drag — is never taken in a smoke.
- **The snapshot gate.** Its whole domain is table-tested, but no smoke arranges a wake in which Neovim is dead, or no pane is bound, and then checks that the layout saved before it survived. Inducing that race under a compositor would be timing-dependent rather than deterministic.
- **The button-width check discriminates only a large regression.** The button is centered, so the left-inset click point misses only below roughly three-quarters of the requested width, and the button's height is undiscriminated entirely.
- **Two crash assertions have no isolated red run.** The overlay-present assertion cannot be reached by a revert that hides the overlay, because the button lives inside it. The unsaved-edit assertion goes red alongside everything else on a broken restart; what it independently discriminates is that the reopened buffer came from the swap file rather than a clean re-read from disk.
- **Forward is never driven end to end**; only Back is. `nav_press` and `nav_buttons` treat the two symmetrically and core's `history_forward` mirrors `history_back`, so what stays undiscriminated is the widget wiring for Forward alone.
- **Nothing asserts that the navigation buttons follow the focused pane at the widget level.** The rule is table-tested in the seam, and OCR cannot read a sensitivity state in any case. Related and equally unmeasured: `refresh_nav_buttons`' deadlock hazard — a read guard held across the nav mutex would hang on a wake, indistinguishable from a slow machine.
- **`nav_press`'s `Refuse` arms are unreached by any integration test.** The seam's table covers the whole domain, but no smoke can arrange a dead, stalled or absent core behind a chord press. The buffer picker's and the search overlay's refusals above are the same shape.
- **The restored `sidebar_visible` reaches no pixel.** No relaunch smoke hides the sidebar, quits and relaunches. The seam is table-tested and the applier demonstrably reads it; the write side is the half that could regress silently, since persisting the live visibility rather than the preference would hide the sidebar after any project-less session.

## Deliberate test gaps

Behavior this front-end has that is deliberately not smoked, with what covers it instead.

- **The primary menu popover.** The keystroke injector cannot click a header-bar menu button, and keyboard-navigating to it is too fragile to be worth the flake. The action namespace's own tests cover every item referencing a declared, implemented, non-reserved action and every label matching the command registry. Eight of the menu's items — New, Go to File…, Command Palette…, Buffer Picker…, Search in project, Close, Back and Toggle sidebar — have their action activated end to end by the accelerator smokes, and the `:qa` smoke performs Close Window's action through the command drain rather than through the menu's own action.
- **The header-bar buttons' click path**, all four: no pointer smoke clicks the sidebar toggle, Back, Forward or Open. Every pointer smoke derives its target from a probe click Neovim reports back and a header-bar button offers no such probe, so the honest alternatives are a theme-owned inset pinned as a second measured constant with nothing to cross-check it, or distorting the buttons to buy hit margin. What covers them instead: the two chord smokes drive the same `gio` actions end to end, and [`nav_ui.rs`](../src/README.md#nav_uirs)'s tables cover the sensitivity and per-pane rules.
- **The native file dialogs.** There is no portal backend under `cage`, so a dialog cannot be driven at all. Their callback bodies are trivial, and the empty state's presentation is smoked.
- **The Open Folder dialog's own path into a re-root**, for the same reason: nothing can drive the dialog. What it settles through is not a gap — an Open Recent palette row records the same pending re-root through the same `chdir_to`, and `open_recent_palette_row_reroots_the_window_under_cage` drives that settle into a re-rooted sidebar end to end.
- **Unsaved-edit recovery as a separate scenario.** `crates/wolf-core/tests/swap_recovery.rs` covers the mechanism headlessly and deterministically. Rather than duplicate it with a flakier compositor version, the crash smoke — which already makes an unsaved edit and kills Neovim — asserts the marker on a pixel after recovery and its absence in the pre-crash control, which adds the one thing a core test structurally cannot have: the front-end driving the reopen itself.

## The `key_from_char` helper

`gdk_key.rs`, `roundtrip_nvim.rs`, and `actions.rs`'s own tests each define a small `key_from_char(c: char) -> gdk::Key` helper, because gdk4-rs has no `Key::from_unicode`. It builds the keyval exactly as GDK would from a Unicode scalar:

```rust
use gdk::glib::translate::from_glib;
fn key_from_char(c: char) -> gdk::Key { unsafe { from_glib(gdk::unicode_to_keyval(c as u32)) } }
```

This is test-side only and does not change `gdk_key`'s contract: printables still carry the exact glyph, and `gdk_key` reads `keyval.to_unicode()` and sets `shift: false` for `Key::Char`.
