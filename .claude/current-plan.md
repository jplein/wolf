# Plan: paint the per-pane statusbar strip in the desktop UI font

## Status

Architect's plan verified and accepted. Coder first (implementation + fonts.conf + docs), then
test-writer (the unit table), then coder again (red runs + full gate). That order INVERTS the usual
test-writer-first sequence deliberately: the table targets functions that do not exist yet, and both
agents would otherwise edit `pane_view.rs` concurrently.

## The geometry claim — VERIFIED from source, and everything rests on it

`statusbar_layout(primary_font_size, secondary_font_size, vertical_padding, line_gap)`
(`statusbar_layout.rs:102`) takes four `f64`s and returns five. Its entire `use` list is
`wolf_core::Rgb` — no Pango, no GTK, no glyph measurement, no text. Every consumer of the strip's
height takes it as a number: `editor_inset::grid_rect(.., self.statusbar.height, ..)`
(`pane_view.rs:1483`), `min_pane_size` (`:1391`, `:2226`), the Cairo clip (`:2434`).

**So changing the FAMILY alone moves no geometry.** `winheight(0) == 11` holds, `web_top == 159`
holds, the 63 px per-pane floor holds. Keep every size.

## The four answers

1. **Seam**: a private `fn ui_font_family(gtk_font_name: Option<&str>) -> Option<String>` in
   `pane_view.rs`, plus a widened `strip_font`, with a unit table in that file's existing
   `#[cfg(test)] mod tests`. NOT `statusbar_layout.rs` — the rule needs
   `pango::FontDescription::from_string` to parse, and that module's stated value is needing neither
   Pango nor GTK nor Cairo. Precedent in the same file: `preview_swap_payload` and its nine-row table.
2. **Read once at `build`** (per tab), no `notify` handler. Consequence, to be stated in the source
   comment: a desktop font change reaches tabs opened after it, and every tab at the next launch.
   Matches `DECISIONS.md:225`, "A config edit therefore takes effect on the next launch".
3. **Width**: `draw_text_line` sets no `width` and no `ellipsize`; `draw_pane` clips to the strip
   band at `:2434`. An overlong path is cut mid-glyph at the pane edge. A proportional face is
   narrower per lowercase character, so the same path clips LATER — marginally more path visible.
   Appearance only, no behaviour change. The clip-instead-of-ellipsis divergence from macOS stays
   and stays documented at `pane_view.rs:2370-2373`.
4. **macOS parity, not a Linux choice.** `PaneContainerView.swift:2526-2529` uses
   `.boldSystemFont(ofSize:)` / `.systemFont(ofSize:)` and `:2542` `.systemFont(ofSize:)`, at sizes
   `CTFontGetSize(metrics.font)` (`:615`) and the scaled floor (`:619-623`) — the system UI family at
   the grid-derived size, which is exactly this design. Change no Swift.

## The finding the scoping missed — fix it FIRST

`crates/wolf-gnome/testdata/fonts.conf` pins `monospace` → `DejaVu Sans Mono` but defines no
`sans-serif` and no `sans` alias, while omitting `/etc/fonts/fonts.conf` and every host font dir.
GTK4's built-in `gtk-font-name` default is `"Sans 10"`, which is what the smokes get under `cage`
with no settings daemon. With no generic rule and no host config, `"Sans"` resolves to whichever
pinned tree fontconfig sorts first — a direct hole in the reproducibility property `build.md:23` and
`tests/README.md:159` both rest on. `flake.nix:930-936` already records the same hazard from the
other direction.

## Implementation order

### 1. `crates/wolf-gnome/testdata/fonts.conf` (coder)

Beside the existing `monospace` alias (lines 27-35), add two `<alias>` blocks of the same shape:
`sans-serif` → `<prefer><family>DejaVu Sans</family></prefer>`, and `sans` → the same. Both, because
whether Pango hands fontconfig `Sans` or `sans-serif` is version-dependent and this file cannot fall
back on the system's `45-generic.conf`. Extend the comment above the alias group to cover all three
and say why: the strip requests the desktop UI family, GTK's built-in default for that is `Sans`, and
an unaliased generic makes the strip's face whatever fontconfig sorts first.

The committed file is a TEMPLATE; the shellHook regenerates the copy `FONTCONFIG_FILE` points at, and
`nix develop --command …` runs the shellHook per invocation, so the verification picks it up with no
extra step.

### 2-4. `crates/wolf-gnome/src/pane_view.rs` (coder)

**2. New private `ui_font_family(gtk_font_name: Option<&str>) -> Option<String>`**, inserted above
`strip_font`'s doc block (~`:3904`). Parse with `pango::FontDescription::from_string`, take
`.family()`, trim, and answer `None` for an absent value, an empty or whitespace-only family, or a
description naming no family (a size-only string like `"11"`). Doc must state, since the signature
cannot:

- It takes the FAMILY and discards the rest. The SIZE, because wolf's size math owns the strip's line
  boxes and adopting the desktop's point size would move `statusbar_layout`'s height. The STYLE and
  WEIGHT words, because the bold-when-active rule is the whole active/inactive signal and a
  `gtk-font-name` of `"Cantarell Bold 11"` must not bold an inactive pane's filename.
- `None` means "no override", not "a default family". The crate names no family of its own
  (`DECISIONS.md:231` records why), so an absent or unusable setting leaves the strip on the base's
  family.
- The trim/empty guard's reason, in the shape `secondary_font_size`'s `is_finite` guard is written.

**3. Widen `strip_font`** (`:3916`) to
`(base, family: Option<&str>, size_px: f64, bold: bool)`. Body: clone `base`; if `family` is `Some`,
`set_family`; then the existing `set_absolute_size` and bold branch, unchanged and in that order.
Rewrite only the doc's first line so it names the three properties applied and says the family is the
optional one. **Keep the `set_absolute_size`-not-`set_size` paragraph (`:3907-3915`) verbatim.**

**4. `build`** (`:3540-3569`): after the `let statusbar = …` block, read
`gtk::Settings::default().and_then(|s| s.gtk_font_name())` once, pass through `ui_font_family`, and
hand `ui_family.as_deref()` to the three `strip_font` calls at `:3567-3569`. `gtk::Settings::default()`
answers `None` with no display and the fallback covers it. If `SettingsExt::gtk_font_name` is not
reachable through the `adw::prelude::*` at `:164`, add `use gtk::prelude::SettingsExt;` or read
`settings.property::<String>("gtk-font-name")` — whichever compiles. Extend the "The strip is measured
ONCE" comment at `:3540-3543` by ONE sentence on the read being per tab. Do not restate DECISIONS.

### 5. The unit table (test-writer) — `mod tests` at `pane_view.rs:4134`

One `#[test] fn statusbar_font_family_table()`, two loops, in the shape
`editor_inset::column_cap_table` uses. No new file, so no wiring problem (`wolf-gnome` is Cargo-only).
No GTK, no display, no Cairo — `from_string` and the setters are pure GLib-level calls.

**Loop 1, `ui_font_family`:** `None`, `Some("")`, `Some("   ")`, `Some("11")` → all `None`.
`Some("Cantarell 11")` → `Some("Cantarell")`. `Some("Inter 11")` → `Some("Inter")`.
`Some("Cantarell Bold Italic 11")` → `Some("Cantarell")` (proves style words are dropped).
`Some("DejaVu Sans")` → `Some("DejaVu Sans")` (proves the parse is Pango's, not
`split_whitespace().next()`).

**Loop 2, `strip_font`**, over a synthetic base (`FontDescription::new()` + `set_family("DejaVu Sans
Mono")` + `set_size(14 * pango::SCALE)`):

| family | size_px | bold | expect family | expect size | expect weight |
| --- | --- | --- | --- | --- | --- |
| `None` | 28.0 | false | `DejaVu Sans Mono` | 28 px absolute | not Bold |
| `Some("Cantarell")` | 28.0 | false | `Cantarell` | 28 px absolute | not Bold |
| `Some("Cantarell")` | 28.0 | true | `Cantarell` | 28 px absolute | Bold |
| `Some("Cantarell")` | 22.4 | false | `Cantarell` | 22.4 px absolute | not Bold |

Assert size through `desc.size()` AND `desc.is_size_absolute()` — the second is what says
`set_absolute_size` was used. `set_absolute_size` stores integer Pango units, so `22.4 * 1024` is not
exact: strict equality on the 28.0 rows, one-unit tolerance on the fractional one, and name the hazard
in the row comment rather than tuning a literal.

### 6. Red runs (coder), before the final gate

| # | edit | expected red | expected green, and why it matters |
| --- | --- | --- | --- |
| 1 | delete `set_family` from `strip_font` | loop-2 rows 2, 3, 4 | row 1 green → the failure names the OVERRIDE. Proves the feature exists |
| 2 | delete `set_absolute_size` | all four rows on size; `is_size_absolute()` false | family/weight green → names the SIZE. The counterpart to the geometry claim |
| 3 | hardcode `set_weight(Bold)` | rows 1, 2, 4 | row 3 green → bold-when-active is not a constant |
| 4 | drop the trim/empty guard in `ui_font_family` | loop-1 rows 2, 3 — **IF** Pango reports an empty family for `""`/`"   "` | **Measure, do not predict.** If they stay green the guard is defensive: record that as an explicit NON-CLAIM in the doc, in the shape of `statusbar_layout.rs:469-473`'s row-6 non-claim, and do NOT delete the rows |

Edit 5 (`build` passing `None`) is not unit-falsifiable — `build` needs GTK. Named so nobody invents a
mock for it.

**The geometry claim is not testable and must not be faked.** No source edit can make the strip's
height depend on the family, because `statusbar_layout` has no family parameter. Its real guard
already exists and already runs: `pane_status_bar_shows_each_panes_filename_under_cage` assertion 7,
`winheight(0) == STATUS_EXPECTED_WINHEIGHT` (`smoke_headless.rs:30379`). It staying green IS the
proof. Add nothing.

**Recommended against — a pixel-proportionality smoke.** `count_exactly` matches an EXACT colour and
the strip's glyphs are `statusbar_text`'s blend with antialiased edges, so an ink count under-counts
by an amount depending on the face under test, making any threshold tuned rather than derived. It also
needs a new band or scenario on a 577 s leg, to prove something the unit table implies for a
thousandth of the cost.

### 7. Docs (coder)

- `tests/README.md:159` — edit the `monospace`-alias sentence IN PLACE to cover the generic sans
  aliases too, so both a grid request and the strip's desktop-UI request are deterministic.
- `src/README.md`, the `#### pane_view.rs` entry (`:544-562`) — ONE bullet in Decisions: the strip
  takes its FAMILY from the desktop's `gtk-font-name` and its two SIZES from the editor's, read once
  per tab, and a family-only override moves no geometry. Link `statusbar_layout.rs`; do not restate
  it. No entry for a private helper. The `statusbar_layout.rs` entry needs no edit.
- `DECISIONS.md`, Typography (`:223-231`) — ONE paragraph after `:231`: the grid takes the configured
  editor family while the strip takes the desktop UI family at wolf's two sizes; this is macOS parity
  with the `PaneContainerView` evidence; and the load-bearing half — the desktop font's SIZE is
  discarded on purpose, because `statusbar_layout` reserves the height from the size alone, so
  adopting it would move the strip height, the grid rect's origin, the pane content floor and every
  measured constant the OCR smokes carry.
- Do NOT touch `architecture.md` — one crate's rule. Do not restate the fontconfig alias in
  DECISIONS; `:19` already delegates that file to `tests/README.md`.
- `prettier --write` all three, then `./tools/lint/markdown_check.sh` on them.

## Existing tests to re-read — expect zero edits, confirm each

- `statusbar_layout.rs:313`, `:475`, `:627` — three unit tables, all over numbers. Confirm, do not edit.
- `smoke_headless.rs:30380` `pane_status_bar_shows_each_panes_filename_under_cage` — assertions 5 and
  6 OCR the strip's text in the new face; most likely to move. Assertion 7 is the geometry guard.
- `smoke_headless.rs:28650` `dragged_divider_weights_survive_relaunch_under_cage` — the narrow pane's
  strip sits directly above the heading it OCRs. Red here means re-measure and note, never weaken;
  `:28602` already forbids tuning against an imaginary constraint.
- `smoke_headless.rs:31197` `drawer_band_rect` — derives from `statusbar_layout(...).height`,
  family-independent. Confirm.
- The preview smokes' `first_uniform_row` scans — `preview_theme_scan_rows` starts INSIDE the strip and
  looks for a colour the strip never paints. `web_top == 159` cannot move. Confirm.
- The 21 negative OCR assertions (`!ocr_text.contains(…)`, `:15949` … `:33553`). The one new failure
  mode: a term previously illegible in a path line becoming legible in a face designed for small
  sizes, flipping an absence assertion red. Investigate as that class; do not weaken.

## What must not change

- The strip's height, its two-line layout, and the five `STATUSBAR_*` layout constants.
- `statusbar_layout.rs` — not one line. No new argument, dependency or test.
- The colour rules: `statusbar_text`'s alphas, `STATUSBAR_INACTIVE_ALPHA`, the blend toward the
  background, the bold-when-active flag. Colours come from the editor's highlight 0.
- The two SIZES. The desktop font's point size is parsed and thrown away.
- The grid rect, the pane origin, `min_pane_size`, `winheight(0) == 11`, `web_top == 159`, the 63 px
  per-pane floor.
- The editor grid, cmdline, toast and completion popup keep the editor font. `render::draw_glyph`,
  `render::paint`, `CellMetrics::for_font` and `draw_cmdline` are untouched.
- No Swift. `PaneContainerView.swift` is read-only evidence.

## Lint

No `#[allow]` expected. Take `Option<&str>`, never `Option<&String>` (`clippy::ref_option_ref`); do not
return `Option<Option<String>>` (`clippy::option_option`).

## Verification

```sh
nix develop --command cargo fmt --all -- --check
nix develop --command cargo clippy --workspace --all-targets -- -D warnings
nix develop --command cargo test -p wolf-gnome
```

During red runs, `cargo test -p wolf-gnome --lib statusbar_font_family_table` is seconds; only the
final green run needs the full ~10.5 minutes.
