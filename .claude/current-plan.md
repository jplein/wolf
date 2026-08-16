# Plan: GNOME-menu appearance for the four wolf-gnome overlays

## Status

COMPLETE. All suites green, three falsifiers captured red, docs corrected, lint clean.

## CORRECTION to this plan's own record

The claim recorded below — "Had the typo taken the whole block, F1 would have read `(29,29,32)` and
gone green like F2" — was true of the PRE-D2 test and is false of the shipped one. Assertion 4 now
carries the `!= PICKOVER_UNSTYLED_FILL` inequality, so a whole-block CSS failure reading `(29,29,32)`
reddens rather than passing. The technical writer caught this when it was handed to it as fact and
refused to write it into `tests/README.md`. The live gap it wrote instead is the real one: assertion 4
reads the fill empirically and only rules colours OUT, so a stylesheet resolving to some fourth
opaque colour passes, and the one inequality that discriminates is pinned to a libadwaita literal
that silently degrades to a no-op if a release re-tints `@view_bg_color`.

## FINAL STATE — seven modified files

`.claude/current-plan.md`, `crates/wolf-gnome/DECISIONS.md`, `crates/wolf-gnome/src/README.md`,
`crates/wolf-gnome/src/overlay_style.rs`, `crates/wolf-gnome/src/picker.rs`,
`crates/wolf-gnome/tests/README.md`, `crates/wolf-gnome/tests/smoke_headless.rs`. No untracked files,
no falsifier leftovers (checked for `*.orig`, `*.bak`, `*.rej`, `*~`).

| gate | result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo build --workspace --all-targets` | pass |
| `prettier --check "**/*.md"` | pass, repo-wide |
| `./tools/lint/markdown_check.sh` on all three edited docs | **no findings**; the only warnings are `[big-file]` and the `pane_view.rs` / `tabs_ui.rs` entry budgets, all proven pre-existing by extracting each file at `HEAD` and diffing the finding sets |
| `cargo test -p wolf-gnome --test smoke_headless` | **70 passed, 0 failed, 3 ignored** |
| `cargo test --workspace` | **exit 0** — wolf-core lib 237, wolf-gnome lib 380, smoke 70, doctests 0 |

## FAILURE EVIDENCE — three falsifiers, all captured red

| falsifier | edit | result |
| --- | --- | --- |
| **F2** | body of `overlay_style::apply` emptied | RED, assertion 4's `PICKOVER_UNSTYLED_FILL`, `left: (29,29,32) right: (29,29,32)` |
| **F3** | `padding: 8px 10px` removed from `list > row` | RED, assertion 9, run of **22** against the 28 floor; assertions 1–8 green |
| **P1b** | the paint-order falsifier from the test's own doc | RED, assertion 4's absence half, `left: 32000 right: 0` |

Every source file restored and confirmed by sha256, not by eye.

## Process findings worth carrying forward

- **`cp -p` is an unsafe revert.** A preserved mtime lets cargo skip the rebuild and re-run the stale
  binary; the coder hit a false red this way and caught it by comparing source mtime against
  `target/debug/deps/*.rmeta`. Restore with plain `cp`, or `touch` after, and look for a `Compiling`
  line before believing a falsifier result. A checksum proves the bytes, not the binary under test.
- **`git diff --stat HEAD -- 'crates/*/src'` is a false-empty pathspec** — it reports nothing even
  with files under it modified. Use `crates/wolf-gnome/src` or `'crates/*/src/*'`.
- **`RUSTDOCFLAGS="-D warnings" cargo doc -p wolf-gnome` is red crate-wide and always was** — 111
  errors, 105 `private_intra_doc_links` plus 6 `crate::liveness_ui` ambiguities. Proven pre-existing
  by stashing only `overlay_style.rs` and diffing the two error sets. Not this change's to fix.
- **`crates/wolf-gnome/tests/README.md` is 1.66x the size ceiling and grows every increment.** A
  structural split is its own task.

## Still unproven, recorded in `tests/README.md`

`@popover_fg_color`, the radius, the shadow, the box padding and the entry/label margin reach no
assertion; light mode reaches no pixel path at all; the `viewport` rule changes no pixel under dark
(measured byte-identical captures) and is kept for the light path. Pre-existing and out of scope:
assertion 7's ink floor still has no captured red.

## FULL GATE — GREEN

| step | result |
| --- | --- |
| `cargo fmt --all -- --check` | pass (the test file's two hunks are fixed) |
| `prettier --check "**/*.md"` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo build --workspace --all-targets` | pass |
| target smoke alone | **ok, 1 passed**, 12.26 s |
| `cargo test -p wolf-gnome --test smoke_headless` | **70 passed, 0 failed, 3 ignored** — matches `tests/README.md` |
| `cargo test --workspace` | **exit 0** — wolf-core lib 237, wolf-gnome lib 380, smoke 70, doctests 0 |

## FAILURE EVIDENCE — fresh, against the FINAL test

- **F2** (body of `overlay_style::apply` emptied) — RED at `smoke_headless.rs:27496`, assertion 4's
  `PICKOVER_UNSTYLED_FILL` line, `left: (29,29,32) right: (29,29,32)`.
- **P1b** (the falsifier written verbatim in the test's own doc) — RED at `smoke_headless.rs:27465`,
  assertion 4's absence half, `left: 32000 right: 0`.

Both files restored by byte copy and confirmed by sha256, not by eye. Target smoke re-run green after
the reverts.

## MEASUREMENTS — both open questions answered, one prediction upheld

- **Open question 4 answered: the box renders exactly 600 × 400.** `box_left = 465`, `box_right =
  1065`; `box_top = 71`, `box_bottom = 471`. `padding: 6px` does NOT fold outward — it eats the
  content area, which measures 588 × 388, and the entry's focus-ring bbox confirms the 6 px inset on
  both sides. The plan's "may render 612 × 412" is withdrawn.
- **Open question 3 measured, prediction holds.** Row height is **34 px** shipped and **22 px** with
  the `list > row` padding removed (delta 12, not 16 — Adwaita's own row padding is 2 px per side).
  Against the measured 348 px viewport that is capacity 10 vs 15: exactly a third fewer, as predicted.
  The heights and the viewport are directly measured; the two counts are a derivation assuming rows
  abut.
- **D7 changes no pixel — measured, not inferred.** Captures with and without the `viewport` rule are
  byte-identical, which also shows the capture is byte-reproducible across runs on this host.

## Coder findings worth keeping

- **`git diff --stat HEAD -- 'crates/*/src'` is a false-empty check** — it prints nothing even with
  `crates/wolf-gnome/src/overlay_style.rs` modified. Earlier passes used it as revert evidence; the
  checksums are what actually carried that claim. Use `crates/wolf-gnome/src` or `'crates/*/src/*'`.
- **`RUSTDOCFLAGS="-D warnings" cargo doc -p wolf-gnome --no-deps` is red crate-wide and always was**:
  111 errors, 105 `private_intra_doc_links` plus 6 `crate::liveness_ui` ambiguities. Proven
  pre-existing by stashing only `overlay_style.rs` and diffing the two error sets — identical, and no
  error mentions `overlay_style`. D3's specific hazard does not fire. Not a blocker and not this
  change's to fix; recorded because the reviewer expected green.

**D8 — the line-height half of the feature gets an assertion.** The user asked for two things and
only the background half is discriminated. The coder measured a falsifiable observable at zero extra
cost: the selected row's highlight is 34 px shipped, 22 px with the row padding removed. A floor
between the two reddens on the feature's removal. Not scope creep — it covers the requested feature.

## REVIEWER VERDICT — approved with minor notes

No logic error found. The thread-local guard is sound across windows and fails in the safe direction
(a second thread would add an identical, inert provider — it cannot fail by SKIPPING an install).
All five selectors match the real widget tree. The four call sites are identical in placement and
nothing else about them diverged. D3 introduces no rustdoc hazard — the reviewer grepped the crate
and no item, public or private, links to `crate::overlay_style`.

`crash_overlay.rs:111` and `shell.rs:577` confirmed deliberate, with a stronger reason than the plan
had: `crash_overlay.rs:109`'s own comment records that `.background` is the opacity mechanism that
makes the scrim hide the frozen last frame instead of tinting it, and `crash_overlay.rs:96-104` warns
that any padding change silently invalidates `restart_button_rect` and the pixel the crash smoke
clicks. Converting that call site would have been a real regression.

One unreachable-but-worth-naming path: if `gdk::Display::default()` were `None` for a process's whole
life the box would be fully transparent rather than falling back to `@window_bg_color` — a worse
failure than the pre-change state. Unreachable because `gtk::Box::new` carries
`assert_initialized_main_thread!()` and GTK cannot initialize without a default display. Recorded so
nobody "fixes" the guard in the wrong direction.

**D7 — the `viewport` node gets added to the transparency selector.** The reviewer showed the
viewport paints nothing in Adwaita dark (the measured `(54,54,58)` could only have reached the
reference band by passing through it), so this is not a live bug. I am adding it anyway: the evidence
covers dark only, light mode reaches no pixel path at all, and a viewport that already paints nothing
cannot be made more opaque — so the change cannot redden any smoke. The reviewer's framing is the
deciding one: leaving it as neither an addition nor a recorded measurement leaves the next reader
unable to tell the omission from an oversight.

## INSET MEASUREMENTS (coder, decode of the retained baseline captures)

R inset by the declared 12 px radius on right and bottom — `x 787..1053, y 159..459`:

| quantity | value |
| --- | --- |
| pixels | 79,800 (266 × 300) |
| `sample_dark` | **0** |
| `sample_ink` | **0** |
| `sample_fill_fraction` | **exactly 1.0000000000** (79,800 / 79,800) |
| non-fill histogram | **empty** — R holds exactly one colour, `(54,54,58)` |
| `control_dark_fraction` | 0.940489 (floor 0.80) |
| `control_ink` | 1,976, unchanged (floor 500) |
| `overlap_w` / `overlap_h` | 266 / 300, both over the 200 floors |

The arc's intrusion has bbox `x 1054..1064, y 460..470` — 38 pixels inside a corner square of side 11.
So a 12 px inset clears it with exactly one pixel of margin per axis, and that margin comes from the
`border-radius` the stylesheet already names rather than from a tuned number. D5 holds, and the whole
arc is outside R, so no shadow-alpha change can move a pixel into it.

**D6 — assertion 6 keeps the TRUE edges; assertions 7 and 8 both take the inset rect.** Two reasons,
both structural. Assertion 7 exists to prove the region assertion 8 samples was a live preview, so the
pair must describe the SAME rectangle or assertion 7's guarantee does not cover assertion 8's region.
And the inset must be applied only AFTER both halves of assertion 6 have run: under P1b
`box_right == web_left == 787`, so insetting first yields `x1 < x0` and panics on an underflow inside
`SampleRect::pixels()` instead of reddening assertion 6 with its diagnostic. Assertion 6 having
already established both spans are ≥ 200 ≫ 12 is what makes the subtraction unconditionally safe.

`PICKOVER_SAMPLE_MIN_FILL_FRACTION` stays 0.98 even though the measured value is exactly 1.0 — the
slack is reserved for a list row that renders taller, per the constant's own doc.

## SECOND MEASURED PASS (coder) — the re-derivation works, one pixel survives

Re-derived edges land exactly where the opaque surface stops: `box_right = 1065`, `box_bottom = 471`,
R back to the documented `787..1065 × 159..471`, 86,736 px. Fill fraction 0.9698 → **0.999562** (38
non-fill pixels). The floor `0.98` stays — the arc costs 0.000438, ~46x under the floor, and that
slack is what the constant's doc reserves for a taller list row.

**`sample_dark` is 1, not 0** — at `(1064, 470)`, R's own bottom-right corner pixel. No value of
`PICKOVER_EDGE_MIN_NON_FILL_PX` in `12..=159` excludes it; every one yields the same edges. The arc's
other exposed pixels read 24/25/26 (shadow-tinted); only the pixel diagonally furthest from the
surface has faded to the preview's exact value.

**F2 — RED at assertion 4's new `PICKOVER_UNSTYLED_FILL` line**, reading `(29,29,32)`, matching the
first pass exactly. The gap D2 targeted is closed: the feature can no longer be deleted while this
smoke stays green.

**P1b — RED at assertion 4's absence half**, which is what the test's own doc predicts (assertion 4
fires first; 6 is never reached). The coder carried assertion 6 through by hand off the retained P1b
captures using the NEW fill-side scan: column 787 carries 160/160 non-fill, so the scan stops on its
own first column, `box_right = 787`, `overlap_w = 0`, well under the 200 floor. **The fill-side scan
is not self-fulfilling** — that was the whole point of the re-derivation and it holds.

Gate after reverts: clippy and build clean; `git diff --stat HEAD -- 'crates/*/src'` empty. Two
`rustfmt` violations remain, both in the test file (`smoke_headless.rs:23756` and `:27421`) — the
test-writer's to fix.

**D5 — R is inset by the declared corner radius, not by one pixel.** All four of the coder's inset
variants are rejected. A 1px inset passes for an accidental reason: the arc's other exposed pixels
escape the `== PREVIEW_DARK_BG` check only because the shadow tints them to 24/25/26, so a change to
the shadow's alpha would redden the assertion for a reason nobody could attribute — the same "green
for a bad reason" shape already caught once in this workflow. The box has a 12px `border-radius`, so
a rectangle spanning the box's full straight extent is genuinely not fully occluded at its corners;
that is geometry, not noise. Assertion 8 samples an R inset by the radius on the right and bottom.
Assertion 6 keeps measuring the TRUE overlap, so it still says the box really does cover the preview
by ≥200×200 — only the fully-occluded sampling region shrinks.

## MEASURED RESULTS (coder, first pass) — these supersede predictions

Gate green (fmt, prettier, clippy `-D warnings`, build). Headless crate tests green: `--lib` 380,
`--doc` 0, `gdk_key` 1, `paint_golden` 16 + 1 ignored, `vendored_pins` 3, `frame_metrics_gate` 4.
Step 4: the fourteen other overlay smokes **14 passed, 0 failed** (142.6 s) — "exactly one test can
see this change" holds. `cage` got a real headless compositor; the pixel evidence is genuine.

**Step 2 RED at assertion 8** (`smoke_headless.rs:27473`), decision-table row 2. Assertions 1–7
passed. `R` = `x 787..1069, y 159..476` (89,394 px) against the documented `787..1065 × 159..471`
(86,736) — `box_right` +4, `box_bottom` +5. `sample_dark` 66, `sample_ink` 0, fill fraction 0.9698
against the 0.98 floor. Fill read back as **`(54,54,58)` = `@popover_bg_color` dark**, as predicted,
and absent from the control. The change reached the pixel it was meant to.

Coder's forensics on the retained captures — both mechanisms present, shadow dominant:

- The opaque surface still ends at **x = 1065, y = 471**. Beyond it is a 4-column / 5-row band of
  `(23,23,23)`–`(26,26,26)`: the outset shadow. It is not *exactly* `PREVIEW_DARK_BG`, so
  `first_uniform_column`/`first_uniform_row` walk straight past it. **2,592 of the 2,696 non-fill
  pixels are that band** — that is what drops the fraction.
- The 66 exact-`(27,27,27)` pixels are a quarter-disc at the bottom-right corner, bbox
  `x 1054..1068, y 464..475` — the 12 px radius.
- Row-scan data for the re-derivation: **rows 460–470 have 1–11 non-fill pixels (the corner); row 471
  has all 278.** Any threshold between 12 and 277 separates them cleanly.

**F1 (typo `@popover_bg_colour`) — RED at assertion 4**, `control_fill_hits` 32000 vs 0, colour
`(20,22,27)` in both captures. Exactly the predicted shape. But narrow: it reddens only *because* the
`list { transparent }` rule outlives the erroring declaration. Had the typo taken the whole block,
F1 would have read `(29,29,32)` and gone green like F2.

**F2 (`apply()` made a no-op) — GREEN. This is the finding.** Green *including* assertion 8. With no
class applied the box still paints an opaque `(29,29,32)` = `@view_bg_color` across the whole
reference band — the `GtkListBox`'s own view background, which the shipped stylesheet deliberately
turns transparent. So assertion 4 is satisfied by the list widget's default, not by anything
`overlay_style` does. **The smoke discriminates "an opaque box covers the preview", not "the overlay
stylesheet resolved".** Deleting the feature makes the test pass.

Working tree confirmed clean after both falsifiers (`git diff --stat HEAD -- crates/` empty,
`overlay_style.rs` sha256 back to `a6b16d53…`).

## ORCHESTRATOR DECISIONS

**D1 — assertion 8 is re-derived, not softened.** Legitimate consequence of accepted work. Option (a)
is adopted but CORRECTED: the coder measured that option (a) as literally written starts the
`box_bottom` row scan at `reference.y1 = 460`, and row 460 already holds a corner pixel, so
`box_bottom` stops at its own scan start and stops measuring anything — a green for a bad reason.
Use the row-population threshold instead (see the measured data above).

**D2 — the F2 gap gets closed, not merely recorded.** The plan's fallback was to record it as
unproven. I am overriding that for one reason: the test is being edited anyway for D1, and one
`assert_ne!` in the same assertion makes the difference between a feature with zero discriminating
coverage and a feature with some. A test nobody has seen fail is attached to nothing, and F2 proved
this one is. The addition takes the same shape as the two `assert_ne!`s already in assertion 4.

**D3 — `mod overlay_style;` stays private.** It is internal glue with no consumer outside the crate
and nothing testable in it; `pub mod` would widen the crate's public surface for no gain. The
rustdoc `private_intra_doc_links` hazard is real only if a *public* item's doc ever links to it —
reviewer confirms none does.

**D4 — the residual gap is recorded.** `@popover_fg_color`, the radius, the paddings and the
entry/label margin still reach no assertion, and F1's narrowness is a real caveat. Both go in
`tests/README.md` under "What is still unproven". That is the technical-writer's.

## The change under review

Committed as `77f8c1d` on top of `94c70ba`. `git diff 94c70ba..HEAD` is the change. Working tree clean.

- `crates/wolf-gnome/src/overlay_style.rs` — NEW. `pub(crate) fn apply(&gtk::Box)` adds a
  `wolf-overlay` class and installs one display-global stylesheet, guarded by a thread-local flag.
- `picker.rs`, `palette.rs`, `buffer_picker.rs`, `search_overlay.rs` — each swapped
  `add_css_class("background")` for `crate::overlay_style::apply(&…_box)`.
- `lib.rs` — `mod overlay_style;` (private; every other module is `pub mod`).
- `src/README.md` — one new `#### overlay_style.rs` entry.

The stylesheet sets `@popover_bg_color` / `@popover_fg_color`, `border-radius: 12px`,
`box-shadow: 0 1px 5px 1px rgba(0,0,0,0.18)`, `padding: 6px` on the box, `margin-bottom: 6px` on a
direct `entry`/`label` child, transparency on `scrolledwindow`/`list`, and `padding: 8px 10px` +
`border-radius: 6px` on `list > row`. Against libadwaita 1.9.0, `@popover_bg_color` is `#ffffff`
light / `#36363a` dark; the previous `.background` was `@window_bg_color`, `#222226` dark.

## Architect's key findings

**Exactly one test can see this change:** `file_picker_covers_the_live_preview_under_cage` in
`crates/wolf-gnome/tests/smoke_headless.rs` — the only capture in the repo taken with a transient
overlay open (`tests/README.md` states this invariant). `paint_golden.rs` is Cairo/Pango only, opens
no display, touches no `CssProvider`: it cannot see this change, and a red there is unrelated.

**The colour change alone should NOT break it.** Its reference band `x 500..700, y 300..460` reads
the fill colour empirically out of the sample capture rather than pinning a theme literal. The band
previously read `(29,29,32)` = `@view_bg_color` (the ListBox's own view background); with
`list { background-color: transparent }` it now reads through to `@popover_bg_color` `(54,54,58)`.
Still one flat colour, still absent from the control capture.

**The real hazard is SHAPE, and assertion 8 measures shape.** `box_right`/`box_bottom` are located by
scanning for the first line that is exactly `PREVIEW_DARK_BG`, so the new outset `box-shadow` (drawn
outside the border box, unclipped) pushes both outward, and the 12px radius makes the bottom-right
corner translucent inside R. Predicted: `sample_fill_fraction` drops to ~0.957, below the
`PICKOVER_SAMPLE_MIN_FILL_FRACTION = 0.98` floor. This is a LEGITIMATE consequence of accepted work.

## Failure-mode decision table (binding)

| Symptom | Diagnosis | Response |
| --- | --- | --- |
| Assertion 8 fails on `sample_fill_fraction` ~0.95–0.97, `sample_dark == 0`, `sample_ink == 0` | Shadow widened R past the opaque surface | Re-derive edges. Do NOT soften the floor — the smoke's own doc forbids it |
| Assertion 8 fails, `sample_dark != 0`, small count (tens) | Rounded corner exposing the preview | Same re-derivation |
| Assertion 8 fails, `sample_dark` large, or fill fraction near zero | Not the stylesheet — paint-order/timing | Investigate; read the retained PNGs |
| Assertion 4 fails: `control_fill_hits != 0`, same colour in both captures | The box painted NO surface — colour did not resolve, or `apply` did not run | Real regression. Investigate the stylesheet |
| Any other smoke fails | None of them can see this change | Suspect a stray `cage`; re-run alone |

**Re-derivation, if assertion 8 fires — option (a), preferred:** locate `box_right` as the first
column at/right of `web_left` whose rows `reference.y0..y1` are NOT `fill`, and `box_bottom`
symmetrically. R then names the opaque surface exactly; `sample_dark` returns to 0 and the fill
fraction to ~1.0. This KEEPS discriminating power: under the P1b paint-order falsifier the column at
`web_left` is not `fill`, so `box_right == web_left`, `overlap_w == 0`, and assertion 6 still reddens.
Re-measure every affected doc number off fresh retained captures, and re-run P1b to prove it.

## No new test — and the two red runs that license that conclusion

`gtk::CssProvider::new()` carries `assert_initialized_main_thread!()`; `gtk::init` appears NOWHERE in
this crate, and GDK4 ships no headless backend. A CSS-parse test would need a compositor, and once
under `cage` the only falsifiable observable is a pixel — which is the smoke that already exists.
Every non-pixel formulation ("the constant contains the string…") passes with `apply()` deleted and
is excluded by this plan's own rule.

The silent-CSS-failure mode is already discriminated, by accident but genuinely. Prove it:

- **F2** — replace the body of `overlay_style::apply` with nothing. Predicted red: assertion 4.
- **F1** — typo `@popover_bg_color` to `@popover_bg_colour` in `OVERLAY_CSS_DATA`. Predicted red:
  assertion 4's `control_fill_hits` half, same colour in both captures.

Revert each after; confirm `git status` clean. **If either comes back GREEN, that is the finding** —
the smoke does not discriminate the overlay's surface, and it gets recorded in `tests/README.md`
under "What is still unproven" rather than papered over.

Gap that remains and reaches no assertion: `@popover_fg_color`, the radius, the shadow, the paddings,
the entry/label margin. No honest cheap test exists (OCR reads neither colour nor geometry). Record
it in `tests/README.md`.

## Commands (all through `nix develop --command`, from the repo root)

Check for a stray `cage` first — a wedged one fails every cage smoke with a lockfile error that reads
exactly like a mass regression.

0. `cargo fmt --all -- --check` · `prettier --check "**/*.md"` ·
   `cargo clippy --workspace --all-targets -- -D warnings` · `cargo build --workspace --all-targets`
1. `cargo test -p wolf-gnome --lib` · `--doc` ·
   `--test gdk_key --test paint_golden --test vendored_pins --test frame_metrics_gate`
2. `cargo test -p wolf-gnome --test smoke_headless -- --exact file_picker_covers_the_live_preview_under_cage --nocapture`
3. the other fourteen overlay-opening smokes (all take their capture after the overlay closed)
4. `cargo test -p wolf-gnome --test smoke_headless` — expect `70 passed`
5. `cargo test --workspace`

Not run: `packaged_layout.rs` / `wolf-package-smoke.sh` (every falsifier there is a `flake.nix` edit),
`bazelisk` (this crate has no Bazel target by design).

## Documentation drift to fix

- **`crates/wolf-gnome/DECISIONS.md` ~line 109** — says a display-global CSS provider is added "one
  per window" for `tree_view`. The crate has TWO providers under TWO policies (`tree_view` unguarded
  per window; `overlay_style` thread-local guarded, once). Rewrite in place, present tense.
- **`tests/smoke_headless.rs` doc comments** — `pickover_fill_reference_band`'s doc says "The box
  paints Adwaita's `.background`", now wrong; `PICKOVER_CONTROL_MIN_DARK_FRACTION`'s doc quotes the
  fill as `(29,29,32)`, which is `@view_bg_color`. Re-measure.
- **`tests/README.md` "What is still unproven"** — add the overlay-chrome bullet.
- Correct as they stand: `src/README.md`, `lib.rs` module doc, `crates/wolf-gnome/README.md`,
  `architecture.md`, `build.md`, `tree_view.rs`'s `install_css` doc.
- `crash_overlay.rs:111` and `shell.rs:577` still call `add_css_class("background")` — deliberate
  (a full-bleed scrim and a pane-filling empty state; neither wants menu chrome). Reviewer confirms.

## Open questions for the reviewer

1. `mod overlay_style;` private vs `pub mod` — the only private module in `lib.rs`. No test needs it
   public. Decide deliberately; leaving it undecided is not an option. Hazard: rustdoc's
   `private_intra_doc_links` fires under `-D warnings` if a public item ever links to it.
2. The `viewport` node between `scrolledwindow` and `list` is not made transparent.
3. Taller rows mean roughly a third fewer visible rows in the three overlays that cap content height.
4. `padding: 6px` folds into GTK's measured minimum, so 600×400 may render 612×412.
5. Light mode reaches no pixel path at all — only dark is verified by any test.
