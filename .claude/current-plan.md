# Plan: change the default line-height to 180% on both platforms

## Status

Architect's analysis accepted. Coder next, in one pass: constants, measurement, doc numbers,
falsifier re-runs, full gate.

## Architect's findings — the shape of the job

**No assertion constant moves.** Every `paint_golden` assertion is RELATIVE — taller-than, equal-to,
centred-within, ink-in-a-band — so none compares a cell height against a literal. What moves is a set
of MEASURED doc-comment numbers plus a few a reader re-derives.

The arithmetic, all of it, so the rest is checkable (`cell_height` is `round(size_px * percent/100)`,
`baseline` is `ascent + (cell_h - ascent - descent)/2`, face metrics unchanged):

| quantity | 130% | 180% |
| --- | --- | --- |
| 16 pt cell / baseline / margin | 28 / 21 / 2 px | **38 / 26 / 12 px** |
| 28 pt cell / baseline | 49 / 37.5 | **67 / 46.5** |
| `sub_baseline_rows` | (39, 49) = 10 rows | **(48, 67) = 19 rows** |
| `wave_band_rows` | (37, 49) | **(46, 67)** |
| `cell_w`, `band_width`, ascent, descent | — | unchanged |

Two facts that bound the risk: the 28 pt baseline shifts by exactly +9.0 with both values on a
half-integer, so the sub-pixel phase is identical and the rasterized decoration should be a pure
9-row translation — a strong prediction, but still a prediction, so the wave rows get MEASURED, not
shifted on paper. And neither new product (38.4, 67.2) lands on a rounding tie, so the Linux/macOS
half-away-from-zero snap cannot diverge here.

Every precondition gains headroom: `:409`'s margin goes 2 px → 12 px, and `leftover_above` 1.0 → 6.0,
so the `baseline == ascent` implementation the case exists to exclude is off by 6 px instead of 1.

## Two doc sites my scoping missed — the architect found them

- `crates/wolf-gnome/src/cell_metrics_math.rs:216-217` — "`wolf_core::default_config()`'s 130, the
  smokes' 118, the golden fixture's 130/100/160". Two false claims after the edit. Rewrite in place
  to 180 and 180/100/160. Pure doc, asserts nothing.
- `crates/wolf-gnome/tests/README.md:83` — "at the shipped 130 % default". Edit in place, then
  `prettier --write`.

Leave `cell_metrics_math.rs:15` and `:113` ("a shared `line-height: 130` gives the same visual
ratio") and `macos-followups.md:40-41`: all three use a shared value to ILLUSTRATE an argument about
arithmetic parity, and that argument is true of any shared value, so they stay correct at 180. Not a
dated-ledger exemption — simply still true. Same reasoning leaves `config.rs:280`.

## Orchestrator rulings

**R1 — the PNG golden stays stale, deliberately.** Nothing reads
`crates/wolf-gnome/tests/testdata/paint_golden.png`: the only references are the `#[ignore]`d writer,
`tests/README.md:90` (which already calls it "a dev aid nothing asserts against") and
`doc-refactor-misc-recommendations.md:95` ("write-only … it gates nothing despite the name"). It will
depict a 52x56 surface where the fixture renders 52x76, and that costs nothing. Regenerating buys a
binary diff in the commit and no coverage. Do not run it, and do not add it to the gate.

**R2 — the coder authors the measured numbers, despite them living in test files.** A measured value
can only come from the run that measured it, and the test-writer cannot run anything. Splitting this
would take three rounds to land numbers one agent already has. The coder records what it measured,
and states what it measured on.

## Problem statement

`wolf_core`'s built-in default line-height moves from 130% to 180%. The unit is already a percentage
of the font size, so "180" is 180% and needs no interpretation.

## What the orchestrator has already established — do not re-derive

- **One constant serves both platforms.** `crates/wolf-core/src/config.rs:24`,
  `const DEFAULT_LINE_HEIGHT: f64 = 130.0`, documented "all platforms". macOS has NO separate
  default: `CellMetrics.swift:90` passes `lineHeightPercent: config.line_height()` straight from the
  FFI config. So the single edit covers macOS and Linux, and **no Swift source change is needed**.
- **The OCR smoke suite is insulated.** `smoke_headless.rs` seeds its own `wolf-config.yaml` with
  `SMOKE_LINE_HEIGHT = "118"` (`:532`, written by `smoke_config_dir`), so no smoke reads the default
  and no measured geometry constant in that file moves. `winheight(0) == 11`, `web_top == 159` and
  every pointer target are safe.
- **These are UNIT examples, not the default, and must NOT be edited**: `config.rs:73` and `:81`
  ("e.g. `130.0` == 130%"), `wolf-ffi/src/lib.rs:866` and `:1470`, `wolf-ffi/src/README.md:205`.
  They explain what the number means. Changing them is churn; leaving them is correct. If the
  technical-writer judges the mismatch confusing, that is its call to make and to justify.
- **These use 130.0 as a pure-function INPUT and must NOT be edited**:
  `crates/wolf-gnome/src/cell_metrics_math.rs:110`, `:119`, `:128`, `:280` and
  `crates/wolf-macos/Tests/WolfKeyMapTests/CellMetricsMathTests.swift:53`. They pin arithmetic, not
  the default, and changing them would weaken tables that currently cover a real configured value.

## Files that must change

1. `crates/wolf-core/src/config.rs:24` — `130.0` → `180.0`.
2. `crates/wolf-core/src/config.rs:254` — `EXPECT_DEFAULT_LINE_HEIGHT`, the deliberate re-typed
   literal that guards the default. Follows to `180.0`.
3. `crates/wolf-ffi/tests/config.rs:47` — same constant, same role, same change.
4. `crates/wolf-core/src/config.rs:138` — the doc naming the built-in defaults ("line-height 130%").
5. `crates/wolf-core/src/config.rs:768` — an assertion message reading "(130%)".
6. `crates/wolf-gnome/tests/paint_golden.rs:45` — `LINE_HEIGHT_PERCENT`. Its doc states it IS the
   value `wolf_core::default_config()` ships, "so the fixture exercises the path a user gets rather
   than a special case". It follows to 180.0, and its measured consequences are the architect's job
   below.

## The one open question — for the architect

**What in `paint_golden.rs` moves when `LINE_HEIGHT_PERCENT` goes 130 → 180, and what are the new
values?** The file carries measured constants and preconditions derived from the cell height at
`FONT_SIZE_PT`, including the fixture precondition at `:409` and the baseline-centring test. Work out
exactly which assertions and which doc-comment numbers depend on it, and say which are re-derived by
arithmetic versus which need a real measurement on this host.

The direction is favourable and should be stated as such: 180% makes the cell TALLER than 130% did,
and `:409`/`:412` want a cell taller than the glyph box — so the precondition holds more strongly
rather than becoming marginal. Confirm that rather than assuming it.

Also check:
- Whether the optional PNG golden (`regenerate_png_golden`, an `#[ignore]`d dev aid) needs
  regenerating, and whether anything in the gate reads it. If it is optional and unread, say so and
  leave it.
- Whether `the_natural_line_height_clamps_a_face_whose_recommended_height_is_short` is affected. It
  uses `NATURAL_LINE_HEIGHT_PERCENT = 0.0`, so my read is no — confirm.
- Whether `crates/wolf-core/src/README.md` or any root document states the default value.
  `macos-followups.md:40-41` mentions "a shared `line-height: 130`" inside a DATED ledger row; my
  ruling is to leave dated ledger entries alone, but say if you disagree and why.

## What must not change

- The unit. It stays a percentage of the font size.
- `cell_metrics_math.rs`'s arithmetic and its tables; `CellMetricsMath.swift` and its tests.
- `SMOKE_LINE_HEIGHT` (118) and every measured constant in `smoke_headless.rs`.
- Any Swift source. macOS picks the new default up through the FFI config with no edit.
- The `CUSTOM_LINE_HEIGHT` (150.0) test fixtures, which exist to prove a parsed file beats the
  default — they must stay DISTINCT from the new default, and 150 ≠ 180, so they hold.

## Verification

```sh
nix develop --command cargo fmt --all -- --check
nix develop --command cargo clippy --workspace --all-targets -- -D warnings
nix develop --command cargo test --workspace
```

`cargo test --workspace` is ~11.3 minutes. The macOS half (`bazelisk test //crates/wolf-macos:...`)
CANNOT be run on this Linux host — say so plainly rather than implying it passed. The Swift-side risk
is bounded by there being no Swift change and no Swift-side default.
