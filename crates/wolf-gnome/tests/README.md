# wolf-gnome/tests

Integration tests for [`wolf-gnome`](../README.md). This crate is Cargo-only by design — it has no Bazel target, so there is no `rust_test` wiring to keep in sync, and `cargo test` discovers every file here.

```sh
nix develop --command cargo test -p wolf-gnome
```

The devShell is not optional: it supplies GTK, exports `WOLF_NVIM_BIN`, and pins the font stack through a generated `FONTCONFIG_FILE`, which is what makes Pango measurement and Cairo rasterization reproducible enough to assert on pixels. The compositor smokes below run in that same default invocation and are the slowest part of it.

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

- Twenty-five end-to-end smokes that launch the built binary under `cage`, drive it with `wtype` and [`wolf-pointer-inject`](../../../tools/wolf-pointer-inject/README.md), screenshot the live window with `grim`, and read it back with `tesseract` in sparse-text mode, plus a second uniform-block reading in the divider-relaunch smoke, which is red without it.
- The scenarios: text rendering, two picker cases, sidebar population, five split-pane cases, five command-drain cases, four pointer cases (click-to-position, drag-to-select including the out-of-widget clamp, wheel scroll, and the click-then-close focus regression), two divider-drag cases (one surviving a re-layout, one a relaunch), two crash-recovery cases, and three application-state cases (a three-pane layout and a lone open file each surviving a quit and relaunch, plus the header bar rendering the working directory).
- Four compositor-free guards on the smokes' own fixtures and harness, which need no toolchain and run in microseconds. The target therefore reports `29 passed`, not `25`.

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

That rule has three mechanical guards, because the fixture and the assertion it can disarm sit a thousand lines apart. One asserts that no sidebar filler filename contains any asserted term; one asserts the same of the temp working directory's two fixed path components, which the header bar renders as page text; and one asserts it of the relaunch fixtures, including the crossed pair the title smoke needs — its project directory must carry the present term and its home directory the absent one, and neither may carry the other. The tables of asserted terms are built from the assertion constants themselves rather than retyped, so they cannot drift, and every term is checked to be lowercase, since the OCR output is lowercased before comparison.

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
- **The stall overlay's presentation.** Core marks a stall only from a synchronous call's timeout arm, and the one this front-end issues runs at launch, before the window is interactive — so a running app cannot raise the overlay at all, and no smoke induces the launch window. The mapping and the messages are table-tested in the seam, and `crates/wolf-core/tests/nvim_liveness.rs` covers the core side against a genuinely wedged Neovim. Faking one would mean adding a blocking call to the front-end purely to satisfy a test, which would also stall the main loop on every redraw against a wedged nvim.
- **Restoring the window size.** `cage` forces the toplevel to the output size, so a restored size never takes visible effect and no screenshot can show one. The seam is table-tested and the window builder demonstrably reads it. Window position is a different matter and is not a gap: it is deliberately never persisted, per [DECISIONS.md](../DECISIONS.md).
- **A drag on a horizontal (stacked) divider.** Both drag smokes drag a vertical one. `set_divider_fraction` never reads a split's orientation, so what a stacked drag would add is the widget side: the orientation mapping into a vertical `gtk::Paned` and that paned's own travel.
- **The allocation guard on `notify::position`.** Nothing can resize the toplevel under `cage`, so the `max != last_max` arm — the one that tells a clamp GTK performed mid-allocation apart from a user drag — is never taken in a smoke.
- **The snapshot gate.** Its whole domain is table-tested, but no smoke arranges a wake in which Neovim is dead, or no pane is bound, and then checks that the layout saved before it survived. Inducing that race under a compositor would be timing-dependent rather than deterministic.
- **The button-width check discriminates only a large regression.** The button is centered, so the left-inset click point misses only below roughly three-quarters of the requested width, and the button's height is undiscriminated entirely.
- **Two crash assertions have no isolated red run.** The overlay-present assertion cannot be reached by a revert that hides the overlay, because the button lives inside it. The unsaved-edit assertion goes red alongside everything else on a broken restart; what it independently discriminates is that the reopened buffer came from the swap file rather than a clean re-read from disk.

## Deliberate test gaps

Behavior this front-end has that is deliberately not smoked, with what covers it instead.

- **The primary menu popover.** The keystroke injector cannot click a header-bar menu button, and keyboard-navigating to it is too fragile to be worth the flake. The action namespace's own tests cover every item referencing a declared, implemented, non-reserved action and every label matching the command registry, and three of the menu's items are proven end to end by the accelerator and quit smokes.
- **The native file dialogs.** There is no portal backend under `cage`, so a dialog cannot be driven at all. Their callback bodies are trivial, and the empty state's presentation is smoked.
- **The sidebar re-root after Open Folder**, for the same reason. The settle predicate is unit-tested, and the re-root reuses the already-smoked search refresh path.
- **Unsaved-edit recovery as a separate scenario.** `crates/wolf-core/tests/swap_recovery.rs` covers the mechanism headlessly and deterministically. Rather than duplicate it with a flakier compositor version, the crash smoke — which already makes an unsaved edit and kills Neovim — asserts the marker on a pixel after recovery and its absence in the pre-crash control, which adds the one thing a core test structurally cannot have: the front-end driving the reopen itself.

## The `key_from_char` helper

`gdk_key.rs`, `roundtrip_nvim.rs`, and `actions.rs`'s own tests each define a small `key_from_char(c: char) -> gdk::Key` helper, because gdk4-rs has no `Key::from_unicode`. It builds the keyval exactly as GDK would from a Unicode scalar:

```rust
use gdk::glib::translate::from_glib;
fn key_from_char(c: char) -> gdk::Key { unsafe { from_glib(gdk::unicode_to_keyval(c as u32)) } }
```

This is test-side only and does not change `gdk_key`'s contract: printables still carry the exact glyph, and `gdk_key` reads `keyval.to_unicode()` and sets `shift: false` for `Key::Char`.
