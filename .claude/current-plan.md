# Implementation plan — typing-latency instrumentation for `wolf-gnome` (`linux-feat/typing-latency`)

## 0. Orchestrator status

- [x] Branch `linux-feat/typing-latency` created from `linux` in the MAIN working directory (`/home/jplein/dev/jplein/wolf`).
- [x] Architect plan (below)
- [x] Tests written — `src/frame_stats.rs` (22 in-file unit tests, implementation stubbed out behind a marker comment) and `tests/smoke_headless.rs` (`frame_stats_report_covers_the_live_typing_path_under_cage`)

### Orchestrator rulings on the test-writer's ambiguities

1. `report_interval`'s one-hour cap is **inclusive**: `"3600"` → 3600 s, `"3600.5"` → default.
2. `is_empty` is **count-based**: `record(bucket, 0)` makes the accumulator non-empty.
3. `Report` and `BucketSummary` keep all fields public with no `#[non_exhaustive]`, so tests build them with struct literals.
4. `summarize(index: u64)` and `Report.index: u64` stand.
5. The smoke may pass the burst text through a `FRAME_STATS_BURST` environment variable alongside the four listed ones.

The coder must not change any assertion in either test body. If an assertion is wrong, report it to the orchestrator instead.

### Orchestrator rulings on the coder's Part-A recommendations

Accepted, for a second test-writer pass:

1. `percentile_p95_never_exceeds_max` is currently unfalsifiable — its stated falsifier (removing the upper clamp) leaves it green, because `ceil(p·n) ≤ n` for every `p ≤ 1.0`. Add a case pinning the clamp against a caller passing `p > 1.0` (`percentile(&[1,2,3], 1.5) == 3`) and correct the docstring so it no longer claims coverage it does not have.
2. Add a `format_report` case whose count and millisecond values overflow their columns (count `123456`, max `12345.67` ms), so a change from "overflow the field" to "truncate" cannot pass unnoticed.
3. Add `crates/wolf-gnome/tests/frame_metrics_gate.rs`: a headless test binary that, with `WOLF_FRAME_STATS` unset, pins the two properties the "costs nothing when off" claim rests on — `enabled()` is false and `begin()` answers `None` — and that `end`, `note_keystroke`, `note_wake`, `maybe_emit` and `flush_final` are no-ops that neither panic nor write. It needs its own test binary because `enabled()` is a process-wide `OnceLock`.

Declined:

4. `summarize_is_total_over_a_zero_count_bucket` — already covered. The coder's red run 4 shows that removing the zero-count skip panics eight of the existing unit tests with a divide-by-zero, which is a test failure, not a silent pass.

Deferred to the technical-writer: the `flush_final` / file-open-failure / multi-window gaps belong in `tests/README.md`'s "What is still unproven", not in a new test.

Housekeeping for the next coder pass: `cargo fmt --all` (two pure reflow hunks in the test module leave the gate red).

- [x] Implementation (Part A, the instrument)
- [x] Failure evidence captured — 4 smoke reds, 13 seam falsifiers, 6 second-pass reds; no test resisted falsification except `percentile_p95_never_exceeds_max`, which was then repaired
- [x] Measurement run — release binary under `cage` + `wtype`, two scenarios x two runs each, before and after
- [x] Repairs — `draw.grid` repaired; `prev.render` indicted and deliberately routed onward

### The measured verdict

Conditions: AMD Ryzen AI MAX+ 395, NixOS, release profile, `cage` 1280x720, iA Writer Mono S 10 pt, a 2068-line prose buffer. Scenario A: one pane, grid 90x34, no preview. Scenario B: two panes, grid 59x34, live Markdown preview. ~1230 keys per run, `WOLF_FRAME_STATS_INTERVAL=2`. Run-to-run spread 0.6 % (A) and 2.2 % (B), so nothing under ~2 % is a signal.

**Indicted, and repaired: `draw.grid`.** R1 fired through `draw.pane` (p95 median 11.51 / 12.16 ms > 8 ms); R2 put 99.2 % of the parent inside `draw.grid`. `render::paint` built a fresh `PangoLayout` — and with it a fresh `PangoContext` font-map lookup — for every cell. It now builds one `GlyphPainter` per paint and re-texts a single layout. Before/after over the identical recipe with the two binaries alternated in one session: A `draw.grid` `count x mean` 496.8 → 412.6 ms/s (−17.0 %), B 176.6 → 147.2 ms/s (−16.7 %); `draw.pane` p95 median −19.7 % (A) and −15.4 % (B). Laying a whole run out as one string is faster still (−28 %) and was rejected: Pango would advance by the face's hinted advance instead of `metrics.cell_w`.

**Indicted, not repaired: `prev.render`.** R3 fired outright — 988 blocking preview-render RPCs at 6.93 ms mean against 1238 keystrokes, 15.6 % of wall clock, pushing `wake` p95 to 9.1 ms and its worst interval to 34 ms. This confirms `DECISIONS.md`'s pre-registered suspicion that the preview render is the only blocking RPC repeating while the user simply types. The only repair is temporal coalescing, whose trailing edge needs a one-shot event-armed timer and a new user-visible preview-latency budget — a design decision this unit does not pre-authorize, and not a draw fix. **Routed onward as its own unit, with these numbers as its evidence.**

**Not indicted:** `wake` in scenario A (p95 0.11 ms), `wake.tree`, `wake.shell`, `wake.state`, `pane.state`, `draw.read` (0.00 ms — `read_arc()` really does copy nothing and there is no contention with the tokio writer), `draw.status`, `draw.cmdline`, `prev.load` (17.1 %, below R2's paying threshold). R4 never fired in either scenario, so the per-wake full-grid repaint is not indicted by the pre-registered rules even though every frame repaints every cell. `draw.pane` remains over R1's 8 ms after the repair (A p95 median 9.50 ms); the remainder is glyph rasterization at ~1.8 µs per glyph, which needs damage tracking or a shaped-glyph cache — each a unit of its own.

**R5 on `pane.wake` in scenario A** is met arithmetically (residual 0.01 of 0.01 ms) and is ruled a non-finding: every term is at or below the report's two-decimal print resolution, so another bucket would print `0.00` too and cannot supply the resolution R5 asks for. Eliminating the entire residual would return 0.1 % of the frame budget.

### Orchestrator rulings on the measurement pass

Accepted, for a third test-writer pass — `GlyphPainter` retains two pieces of state across cells, and neither is pinned:

1. `crates/wolf-gnome/tests/paint_golden.rs`: an undecorated cell placed **after** the decorated columns, pinning that a decoration does not leak forward. The coder demonstrated the gap — removing `set_attributes(None)` leaves all 13 golden tests green, because `CONTROL_COL = 0` sits before every decorated column.
2. The same file: a bold/italic face switch in both directions within one `paint`, pinning that the face is restored when `(bold, italic)` changes back.

Deferred to the technical-writer: the `flush_final` / file-open-failure / multi-window / `Scope`-drop-enabled gaps belong in `tests/README.md`'s "What is still unproven".

### Orchestrator rulings on the golden-case pass

All three golden cases passed first time against the shipped `GlyphPainter`, and all four falsifying edits reddened exactly the intended cases while the 13 pre-existing golden tests stayed green. Two hardening items accepted for a fourth test-writer pass:

1. `MIN_SLANT_SPREAD_PX` sits exactly on the measured value (floor 3 against a measured 3–4 drift, zero margin), so one rasterizer nudge reddens a correct build. Lower it to 2 — still strictly above `MAX_UPRIGHT_SLANT_PX = 1`, and the falsifier drives the drift to 0 — and put the measured range in its doc comment in place of the derived estimate.
2. `bold_is_applied_and_then_restored_within_one_paint`'s restore assertion is one-sided and would pass over a cell that drew nothing. Add a lower bound tolerant of the ~28 px sub-pixel-phase difference between two identical plain cells.

Declined: a bold-first fixture to pin `face`'s `None`-vs-`(false, false)` initial state. That is a "set the description once rather than twice" performance claim, not a pixel property, and no assertion could see it.

### Orchestrator rulings on the review

The reviewer approved the implementation, verified both count identities and both `GlyphPainter` state-clearing paths against the source, and found no correctness defect. Four items to clear:

1. **`install()`'s `WOLF_FRAME_STATS_FILE` emptiness filter** is a decidable rule sitting in glue, untested, and internally inconsistent (it trims to test emptiness but hands the untrimmed string to `open`). Move it into `frame_stats.rs` as a seam function with a table test, trimming consistently. The seam, not deletion, because macOS filters an empty path too and parity is the point.
2. **`Bucket`'s explicit discriminants** duplicate `index()` with only one of the two pinned. Drop the discriminants; `index()` is the storage index and the test pins it.
3. **`pane_view::PaneHandle::on_render_wake` is reachable from the restart path** (`shell.rs`'s `restart_nvim`, via the deferred one-shot timer and the overlay's Restart button), outside every `wake` region — so a crash-restart lands a `pane.wake` sample with no parent. The seam's module-doc count-identity bullet names only the launch path; it must name this too.
4. **The `#[allow(clippy::too_many_arguments)]` on `draw_glyph`** corresponds to no firing rule (seven arguments; the default threshold fires above seven). Delete it and confirm clippy stays green. The sibling allow on `GlyphPainter::draw` is justified and stays.

Handed to the technical-writer, beyond plan §8: `src/README.md`'s `render.rs` entry needs the `GlyphPainter` decision; `tests/README.md` needs a `#### frame_metrics_gate.rs` entry and the three new `paint_golden.rs` cases; and the "What is still unproven" list gains the `debug_assert!` thread tripwire, the `WOLF_FRAME_STATS_FILE` filter, and `maybe_emit`'s `is_empty()` re-baseline branch.

Worth noting, no action: `GlyphPainter::new(cr)` and `draw(cr, …)` take the context twice with nothing tying them together — both call sites are correct, and the invariant belongs in the doc comment.

- [x] Review — approved with minor notes; all four cleared
- [ ] Docs
- [ ] Merge into `linux`

## 1. Summary

`wolf-gnome` gets an off-by-default frame-time measurement surface for the keystroke-to-pixel path, split the way this crate splits everything: a pure, GTK-free seam (`src/frame_stats.rs`) that owns bucket identity, accumulation, percentiles, report formatting, environment parsing and the interval-elapsed decision, unit-tested in an in-file `#[cfg(test)] mod tests`; and a thin process-global glue module (`src/frame_metrics.rs`) that owns the once-only enabled read, the clock, the accumulator, and the writes to stderr and an optional file. The report cadence rides the existing render wake plus one flush at process exit, so the crate's no-polling property holds with the flag set and unset. The bucket set is derived from the GTK path and is deliberately not a transliteration of the macOS list. Once the instrument exists it is used, under `cage`, against the release binary; repairs are decided from the numbers by a pre-registered procedure, and "nothing is indicted, ship the instrument alone" is an authorized outcome.

## 2. Design decisions (these go into `DECISIONS.md`)

### 2.1 No timer; the emit rides the wake, plus one exit flush

`crates/wolf-gnome/DECISIONS.md` "Threading and the wake" states that nothing polls and that the only two timers are one-shot and event-armed. A `glib::timeout_add` report period would break that property with the flag set.

**Decision.** The emit check is the last statement of the drain-loop body in `src/window.rs` (the `while wake_rx.recv().await.is_ok() { … }` future at ~line 741), _after_ the `wake` bucket's timing region closes, so the emit's own cost is never inside a measured region. One additional, non-periodic flush runs in `app::run` after `adw::Application::run_with_args` returns.

**Consequences, stated honestly.** A report covering the final interval before the application goes idle is not emitted until the next wake. The exit flush covers a clean quit; a process killed with a signal (which is what every `cage` smoke's teardown does) never runs it. That is why `WOLF_FRAME_STATS_FILE` is appended **per report** rather than buffered to exit. The instrument arms no timer and no idle source in either state.

With N windows there are N drain loops sharing one process-global accumulator; whichever window wakes first after the interval elapses emits the process-wide report. This matches macOS, where `FrameMetrics` is process-scoped.

### 2.2 Environment names: parity, not divergence

**Decision.** Reuse the macOS spellings exactly: `WOLF_FRAME_STATS`, `WOLF_FRAME_STATS_INTERVAL`, `WOLF_FRAME_STATS_FILE`, with the same accepted values (`1`/`true`/`yes`/`on`, case-insensitive after trimming), the same 5.0 s default interval, the same `wolf-frame-stats ` line prefix and the same byte-exact table layout.

**Reason.** One documented surface serves both front-ends, and the two report tables become directly `diff`-able as text, which is the only cheap way to compare where the two front-ends spend a frame.

**One deliberate divergence, at the seam boundary rather than the surface.** The seam's parsers take the raw value (`Option<&str>`), not an environment map — the crate's existing rule (`config_seam::resolve_font_pt` takes the override as an argument, so a parallel `cargo test` suite never touches the process environment).

### 2.3 The bucket set — derived from the GTK path

**Two families, and this is the single most important thing about the table.** `queue_draw` only marks a widget dirty. GTK invokes the draw funcs from the frame clock _after_ the wake handler has returned, so the `draw.*` buckets are **not** nested inside `wake`. They are a second top-level family.

| idx | label         | wraps exactly                                                                                  | nesting                                                                              |
| --- | ------------- | ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| 0   | `wake`        | the whole drain-loop body in `window::build`'s `spawn_future_local` future                     | parent of 1–3; residual = `shell.on_liveness_wake()` plus loop overhead               |
| 1   | `wake.tabs`   | `tabs.on_render_wake()` in that body                                                           | in `wake`; disjoint from 2, 3                                                        |
| 2   | `wake.tree`   | `tree.on_render_wake()` in that body                                                           | in `wake`                                                                            |
| 3   | `wake.shell`  | `shell.on_render_wake()` in that body                                                          | in `wake`; parent of 4                                                               |
| 4   | `wake.state`  | `coordinator.commit(self.inner.window_id)` in `shell::AppShell::on_render_wake` (~line 1529)   | in `wake.shell`                                                                      |
| 5   | `pane.wake`   | the body of `pane_view::PaneHandle::on_render_wake` (~line 3253)                               | in `wake.tabs`, once per refreshed tab; parent of 6, 7                               |
| 6   | `pane.state`  | `self.refresh_pane_state()` in `PaneInner::on_render_wake` (line 593)                          | in `pane.wake`; disjoint from 7                                                      |
| 7   | `pane.prev`   | `self.refresh_previews()` (line 598)                                                           | in `pane.wake`; parent of 8, 9                                                       |
| 8   | `prev.render` | the `core.render_preview(load.source_bufnr)` expression alone (line 1621)                      | in `pane.prev`; disjoint from 9                                                      |
| 9   | `prev.load`   | the `self.load_preview_document(id, &html, &source_path, update)` call (line 1623)             | in `pane.prev`                                                                       |
| 10  | `draw.pane`   | the whole body of `PaneInner::draw_pane` (line 2253)                                           | top-level of the draw family; parent of 11, 12, 13                                   |
| 11  | `draw.read`   | the `core.read_snapshot()` expression alone (line 2258)                                        | in `draw.pane`                                                                       |
| 12  | `draw.grid`   | the `paint(cr, grid, &snap.highlights, &self.metrics)` expression (line 2354)                  | in `draw.pane`; recorded only for a non-preview pane                                 |
| 13  | `draw.status` | the statusbar block from `cr.save()` to `cr.restore()` (lines 2301–2332)                       | in `draw.pane`; recorded for preview panes too                                       |
| 14  | `draw.cmdline`| the whole body of `PaneInner::draw_cmdline` (line 2724)                                        | top-level of the draw family, sibling of `draw.pane`                                 |

Disjointness and count relations (these belong in the seam's docs):

- `{wake.tabs, wake.tree, wake.shell}` are mutually disjoint; sum bounded above by `wake`.
- `{pane.state, pane.prev}` are mutually disjoint inside `pane.wake`, with a residual that is `apply_intents`, `detect_closes`, `resolve_pending_grids`, `apply_drawer_frame`, `sync_pane_grids`, `update_active_pane`, `refresh_message_surfaces` and `queue_redraw`. **A residual over half of `pane.wake` is itself a reading**: the table lacks resolution there, and the answer is another bucket and another run, not a repair.
- `{draw.read, draw.grid, draw.status}` are mutually disjoint inside `draw.pane`, with a residual that is the whole-allocation background fill, the geometry, the cursor and the scrollbar thumb.
- `count(draw.status) ≥ count(draw.grid)`: the strip is never skipped and the grid is skipped for a preview pane.
- `count(wake) == wakes` **exactly**, by construction.
- `count(wake.tabs) ≤ count(wake)`: the tab layer runs only when liveness says painting is worth anything.
- `count(pane.wake)` may exceed `count(wake.tabs)` by the launch path's own synchronous first pass (`window.rs` lines 774–776), which is deliberately **not** instrumented.
- `count(draw.pane)` is not `count(wake) × panes`: GTK coalesces frames.

**Deliberately omitted, with reasons**: no bucket for `shell.on_liveness_wake` (an atomic read plus a bool compare), none for the cursor draw (one glyph), none for `refresh_message_surfaces`, and **no per-cell bucket inside `render::paint`** — two clock reads per cell would cost more than the thing measured.

**No instrumentation is placed inside a pure seam.** `render.rs`, `cursor.rs`, `glyph_style.rs` stay glue-free, because `paint_golden.rs` drives `render::paint` directly.

**`draw.read` is not the macOS `pane.read`.** macOS's bucket times a deep clone across FFI; here `WolfCore::read_snapshot` is a `parking_lot` `read_arc()` that copies nothing. A large `draw.read` means lock contention with the tokio writer, never copying.

### 2.4 The `wakes/key` ratio

Emitted in the footer, in the macOS spelling, carrying the macOS caveat: a counter ratio and therefore an **upper bound** on wakes attributable to typing, since mouse, scroll, resize and out-of-band core notifications inflate it. One Linux-specific note for the measurement docs: under the headless recipe there is no pointer and the toplevel cannot be resized, so the only inflation left is out-of-band core notifications — closer to attribution there, and still not attribution.

### 2.5 Cost when disabled, and where the accumulator lives

- `frame_metrics::enabled()` reads a `static ENABLED: OnceLock<bool>`: one atomic acquire load and a branch. Every hot-path function is `#[inline]` and tests it first.
- The once-only environment read is forced by `frame_metrics::install()`, called as the **first statement of `app::run`**.
- The accumulator lives in a `thread_local!` `RefCell<Option<Recorder>>`; no lock, nothing needs `Send`. The slot is `None` unless `install()` found the flag set, so a disabled process allocates nothing.
- **`thread_local!` means a `record` from a non-main thread accumulates into its own slot that nothing reports — silent data loss.** No current call site can reach a non-main thread. The tripwire is a `debug_assert!` comparing `std::thread::current().id()` against a `static REPORTER_THREAD: OnceLock<ThreadId>` stamped by `install()`. Accepted rather than prevented, and documented.
- This collides with an existing crate rule that must be resolved in writing: `coordinator.rs`'s doc comment says "there is deliberately no `static`, no `OnceLock` and no `thread_local!` in this crate", and `app.rs` plus `src/README.md` say the crate has no process-global state. **The rule's purpose is that two `WolfCore`s in one process are independent**, and diagnostic counters no application behaviour reads do not threaten that. Narrow all three statements to _application_ state and point them at the new section; do not delete them.

### 2.6 Recording must never change what is painted, nor take the process down

- `begin`/`end`/`scope` touch no Cairo context, no widget and no core.
- **Do not use `eprintln!` for report output** — it panics if writing to stderr fails, the Rust equivalent of the `NSException`-on-`EPIPE` hazard. Write with `std::io::stderr().write_all(bytes)` and discard the `Result` with `let _ = …`; same for the file handle.
- The emit performs a synchronous main-thread write, once per interval, outside every measured region, adding no `block_on_rpc`.

## 3. Files to create

### 3.1 `crates/wolf-gnome/src/frame_stats.rs` — the pure seam

GTK-free, Cairo-free, Pango-free, environment-free. `std` only. Module doc states the two families, the double-count trap, the count identities from §2.3, and that the seam never reads a clock.

```rust
pub const LINE_PREFIX: &str;                 // "wolf-frame-stats "
pub const DEFAULT_INTERVAL_SECS: f64;        // 5.0
pub const SAMPLE_CAPACITY: usize;            // 4096
pub const MAX_LABEL_WIDTH: usize;            // 13

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bucket { Wake, WakeTabs, WakeTree, WakeShell, WakeState,
                  PaneWake, PaneState, PanePrev, PrevRender, PrevLoad,
                  DrawPane, DrawRead, DrawGrid, DrawStatus, DrawCmdline }

impl Bucket {
    pub const ALL: [Bucket; 15];
    #[must_use] pub fn index(self) -> usize;        // dense, ascending, storage index
    #[must_use] pub fn label(self) -> &'static str; // report-table name
}

pub struct FrameStats { /* [Accumulator; Bucket::ALL.len()], keystrokes, wakes */ }
impl Default for FrameStats { … }               // clippy::new_without_default
impl FrameStats {
    #[must_use] pub fn new() -> Self;            // reserves SAMPLE_CAPACITY per bucket
    pub fn record(&mut self, bucket: Bucket, nanos: u64);
    pub fn record_keystroke(&mut self);
    pub fn record_wake(&mut self);
    pub fn reset(&mut self);                     // zeroes counters, keeps allocations
    #[must_use] pub fn is_empty(&self) -> bool;  // no sample and both counters zero
    #[must_use] pub fn retained_sample_count(&self, bucket: Bucket) -> usize;
    #[must_use] pub fn summarize(&self, index: u64, elapsed_secs: f64) -> Report;
}

pub struct BucketSummary { pub bucket: Bucket, pub count: u64, pub mean_nanos: u64,
                           pub p50_nanos: u64, pub p95_nanos: u64, pub max_nanos: u64,
                           pub truncated: bool }
pub struct Report { pub index: u64, pub elapsed_secs: f64, pub buckets: Vec<BucketSummary>,
                    pub keystrokes: u64, pub wakes: u64, pub wakes_per_keystroke: Option<f64> }

#[must_use] pub fn percentile(sorted: &[u64], p: f64) -> u64;
#[must_use] pub fn format_report(report: &Report) -> String;
#[must_use] pub fn is_enabled(raw: Option<&str>) -> bool;
#[must_use] pub fn report_interval(raw: Option<&str>) -> Duration;
#[must_use] pub fn interval_elapsed(since_last: Duration, interval: Duration) -> bool;
```

Rules the implementation must satisfy:

- `count`, sum and `max` exact and unbounded; only the percentile window bounded to the last `SAMPLE_CAPACITY` samples per bucket, via a ring with a write index. `truncated == (count > SAMPLE_CAPACITY)` — strictly greater.
- `mean_nanos` is truncating integer division `sum / count`; `summarize` omits every zero-count bucket.
- `percentile` is nearest-rank over an already-sorted slice: `idx = clamp(ceil(p * n) as isize - 1, 0, n - 1)`, `0` for an empty slice.
- `summarize` is the only place anything is sorted, once per report.
- `wakes_per_keystroke` is `None` when `keystrokes == 0`.
- `format_report` reproduces the macOS layout byte for byte: prefix on every line including the last; header `#{index}  window={elapsed:.1}s`; a column row `bucket        count     mean      p50      p95      max`; one row per bucket sorted by `index()` regardless of `Report.buckets` order, label left-14, count right-5 with a `*` suffix inside the field when truncated, then mean/p50/p95/max right-9 each as milliseconds with two decimals; footer `keys=… wakes=… wakes/key=…` with `n/a` for `None`. Every line ends in `\n`.
- `report_interval` guards before `Duration::from_secs_f64`, which **panics** on a non-finite, negative or overflowing value: accept only finite, strictly positive, at most one hour; anything else gives the 5 s default. Trim whitespace first.
- `interval_elapsed(since, interval)` is `since >= interval` — inclusive at the boundary.
- **No `delta` function.** `std::time::Instant::saturating_duration_since` already is one. Record this as a non-port.
- Nanoseconds via `u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)`, never an `as` cast.

### 3.2 `crates/wolf-gnome/src/frame_metrics.rs` — the process-global glue

`std` only; no GTK.

```rust
pub fn install();                                     // first statement of app::run
#[must_use] pub fn enabled() -> bool;                 // OnceLock<bool>, #[inline]
#[inline] #[must_use] pub fn begin() -> Option<Instant>;
#[inline] pub fn end(bucket: Bucket, start: Option<Instant>);
#[inline] #[must_use] pub fn scope(bucket: Bucket) -> Scope;
#[inline] pub fn note_keystroke();
#[inline] pub fn note_wake();
pub fn maybe_emit();                                  // last statement of the drain body
pub fn flush_final();                                 // after Application::run returns
pub struct Scope { /* bucket + Option<Instant> */ }   // impl Drop records
```

- `install()` reads `WOLF_FRAME_STATS` once through `frame_stats::is_enabled`, returns immediately when false. When true it reads `WOLF_FRAME_STATS_INTERVAL` and `WOLF_FRAME_STATS_FILE`, stamps the reporting thread id, opens the file create/append (a file that cannot be opened degrades silently to stderr-only), builds the `Recorder`, and writes the header line `wolf-frame-stats enabled interval=<x.x>s`.
- `Scope`'s `Drop` calls `end`. **Bind it to a named `_guard`, never `let _ = scope(..)`**, which drops instantly and measures nothing — put that warning in the module doc.
- `maybe_emit()`: return unless enabled; take `Instant::now()`; borrow the recorder; if `!interval_elapsed(now - interval_start, interval)` return; if `stats.is_empty()` re-baseline and return **without** summarizing; otherwise bump `report_index`, `summarize`, `reset`, re-baseline, **drop the borrow**, then format and write.
- `flush_final()` is `maybe_emit()` with the interval check skipped; still silent when nothing was recorded.
- Writes go to `stderr().write_all` and the optional `File::write_all`, both with the `Result` discarded. Never `eprintln!`.

## 4. Files to modify

| File                                       | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| ------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/wolf-gnome/src/lib.rs`              | Declare `pub mod frame_metrics;` and `pub mod frame_stats;` in the alphabetical module list (between `editor_inset` and `geometry`). Add one bullet for `frame_stats` to the module-doc seam list; no re-exports.                                                                            |
| `crates/wolf-gnome/src/app.rs`              | `run()`: `frame_metrics::install();` as the first statement; bind the exit code from `app.run_with_args::<&str>(&[])` to a local, call `frame_metrics::flush_final()`, return the code. Narrow the module doc's "no process-global state" claim to application state.                        |
| `crates/wolf-gnome/src/window.rs`           | Drain future (~741): `note_wake()` first, then a block holding `scope(Bucket::Wake)` around the existing four statements, with `scope(WakeTabs)` / `scope(WakeTree)` / `scope(WakeShell)` blocks around the three calls, then `maybe_emit()` **after** the `wake` block closes. Leave the launch-path calls at 774–776 and 810 uninstrumented with a one-line comment. `wire_keys` (~857): `frame_metrics::note_keystroke();` immediately after `core.send_input(InputEvent::Key(event));`. |
| `crates/wolf-gnome/src/shell.rs`            | `AppShell::on_render_wake` (~1528): `let _guard = frame_metrics::scope(Bucket::WakeState);` as the first statement inside the `if let Some(coordinator) = …` block.                                                                                                                         |
| `crates/wolf-gnome/src/pane_view.rs`        | Ten call sites exactly as tabulated in §2.3.                                                                                                                                                                                                                                                |
| `crates/wolf-gnome/src/coordinator.rs`      | Narrow the `CoordinatorInner` doc comment claiming the crate contains no `static`/`OnceLock`/`thread_local!` to application state; link to the new `DECISIONS.md` section.                                                                                                                   |
| `crates/wolf-gnome/tests/smoke_headless.rs` | Add `FRAME_STATS_SCENARIO` and one `#[test]` (§6.2).                                                                                                                                                                                                                                        |

Use `begin`/`end` where the region produces a value (`read_snapshot`, `paint`, `render_preview`); use `scope` where the region is a statement list with early returns.

## 5. Dependencies

None. `std` only. No lock-file movement.

## 6. Test plan

Both new suites run under `nix develop --command cargo test -p wolf-gnome`. **There is no Bazel wiring to add and none may be added**: `crates/wolf-gnome/tests/README.md` and `DECISIONS.md` "Platform and build" both state this crate deliberately has no Bazel target.

### 6.1 `src/frame_stats.rs` — `#[cfg(test)] mod tests`

| Test                                                | Behaviour pinned                                                                                                                     | Falsifying edit                                        |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------ |
| `bucket_indices_and_labels_table`                    | each variant's `index()` and `label()`; `Bucket::ALL` is `0..15` ascending with no gaps                                               | change `DrawGrid`'s label to `"draw.cells"`            |
| `bucket_labels_are_unique_and_fit_the_report_column` | no two labels equal; every label ≤ `MAX_LABEL_WIDTH`                                                                                  | make `WakeTree` return `"wake.tabs"`                   |
| `is_enabled_table`                                   | `None`, `""`, `"   "`, `"1"`, `"true"`, `"TRUE"`, `"Yes"`, `"on"`, `" on "`, `"0"`, `"false"`, `"no"`, `"off"`, `"maybe"`             | drop the `trim()` or the lowercase                     |
| `report_interval_table`                              | `None`→5 s; `""`→default; `"2.5"`→2.5; `"10"`→10; `"0"`, `"-1"`, `"abc"`, `"inf"`, `"NaN"`→default                                    | delete the `v > 0.0` guard                             |
| `interval_elapsed_table`                             | below → false, exactly equal → **true**, above → true                                                                                 | change `>=` to `>`                                     |
| `basic_accumulation_pins_integer_mean`               | 10+20+31 over 3 → mean 20, max 31, not truncated                                                                                      | round instead of truncating                            |
| `single_sample_collapses_to_one_value`               | mean = p50 = p95 = max = the sample                                                                                                   | as above                                               |
| `percentile_nearest_rank_table`                      | empty→0; `[7]`; `[10,20]` p50→10, p95→20; `[10,20,30]`; all-equal; `1..=100` p50→50, p95→95, p100→100, p0→1                            | change `ceil` to `floor`, or drop the `- 1`            |
| `percentile_p95_never_exceeds_max`                   | sweep n=1..50: min ≤ p50 ≤ p95 ≤ max                                                                                                  | remove the upper clamp                                 |
| `recording_one_bucket_leaves_others_empty`           | zero-count buckets omitted from `summarize`                                                                                           | delete the `count == 0` skip                           |
| `every_bucket_round_trips_through_record_and_summarize` | one distinct sample per bucket, no cross-bleed, one row each                                                                       | make two variants return the same `index()`            |
| `ring_bound_keeps_exact_count_and_windowed_percentiles` | 100 tiny then 4096 large: count and max exact, `truncated`, p50/p95 from the last 4096 only, mean over the exact sum               | keep the _first_ capacity samples instead of the last  |
| `exactly_capacity_samples_is_not_truncated`          | `truncated` is `count > CAPACITY`, not `>=`                                                                                           | change `>` to `>=`                                     |
| `retained_sample_count_below_capacity`               | retained == count below capacity; 0 for an untouched bucket                                                                           | as above                                               |
| `wakes_per_keystroke_table`                          | 0 keys → `None` (with and without wakes); 30/10 → 3.0; 0/5 → 0.0; 142/48                                                              | relax the `keystrokes > 0` guard                       |
| `reset_zeroes_everything`                            | after every mutator, `reset` empties buckets and both counters                                                                        | drop `samples.clear()`                                 |
| `reset_starts_a_fresh_window_with_no_leakage`        | large pre-reset samples never reach the post-reset window                                                                             | drop the write-index reset                             |
| `is_empty_tracks_all_three_mutators`                 | fresh → empty; after each mutator → not empty; after `reset` → empty. The emit gate's contract                                        | make `is_empty` ignore the keystroke counter           |
| `format_produces_the_exact_pinned_table`             | byte-exact table from a `Report` whose buckets are supplied **out of order**                                                          | change any width by one, drop the `*`, or drop the sort |
| `format_emits_header_and_footer_with_no_buckets`     | header + column row + footer with `wakes/key=n/a`                                                                                     | print `0.00` for the `None` ratio                      |
| `every_formatted_line_carries_the_grep_prefix`       | every line prefixed, trailing newline, line count == buckets + 3                                                                      | drop the prefix from the footer                        |
| `formatted_bucket_rows_are_in_index_order`           | recorded in reverse, emitted ascending                                                                                                | remove the sort in `format_report`                     |

### 6.2 The `cage` smoke — `frame_stats_report_covers_the_live_typing_path_under_cage`

**Why it exists.** `WOLF_FRAME_STATS_FILE` is the one seam a test can actually read, and reading it is the only honest evidence that the buckets are wired into the live keystroke-to-pixel path.

**Modelled on** `window_renders_typed_text_under_cage` for the scenario skeleton, minus `grim` and OCR; and on `pane_layout_survives_relaunch_under_cage` for asserting on a file the running application wrote.

**Helpers used**, all existing: `CAGE_LOCK` (held for the whole body), `require_smoke_env()`, `make_temp_cwd()`, `env!("CARGO_BIN_EXE_wolf-gnome")`, and `cage_command(FRAME_STATS_SCENARIO, &cwd)` — the only permitted spawn point — with `.env("WOLF_BIN", bin)`, `.env("WOLF_FRAME_STATS", "1")`, `.env("WOLF_FRAME_STATS_INTERVAL", "1")`, `.env("WOLF_FRAME_STATS_FILE", &stats_path)`. The report file goes under the resolved `XDG_RUNTIME_DIR` with a name unique to this smoke; a stale copy is removed before the run.

**Scenario** (`FRAME_STATS_SCENARIO`, carrying the standard `WAIT_TRIES`/`WAIT_INTERVAL`/`SETTLE=1.0` block and the canonical pointer back to `SCENARIO`'s measured margin table): launch, poll, settle, then four bursts of `wtype -- "i<~20 chars>"` + `wtype -k Escape` separated by `sleep 1.2`, so at least two report intervals close with keystrokes inside them; a final `sleep`; then the usual `set +e` teardown. No `grim`, no `tesseract`, no fixture files, no pointer helper.

**Asserts**, in this order, each with a message naming the likely cause:

1. The scenario exited 0.
2. The report file exists and is non-empty.
3. It contains a line beginning `wolf-frame-stats enabled interval=`.
4. It contains at least one report header line matching `wolf-frame-stats #1  window=`.
5. Every line begins with `wolf-frame-stats `.
6. Summed over every footer, `keys=` is at least 20.
7. Summed over every footer, `wakes=` is greater than zero.
8. Every one of these labels appears as a bucket row in at least one report: `wake`, `wake.tabs`, `wake.tree`, `wake.shell`, `wake.state`, `pane.wake`, `pane.state`, `pane.prev`, `draw.pane`, `draw.read`, `draw.grid`, `draw.status`, `draw.cmdline`.
9. Neither `prev.render` nor `prev.load` appears anywhere in the file — the non-vacuity half of assertion 8.
10. Every bucket row parses: a positive integer count and four millisecond fields that parse as `f64` and are `>= 0.0`.
11. Within each report block, `count("wake") == wakes`.

**Must not assert**: any wall-clock magnitude, any particular count, the ratio's value, the number of reports beyond "at least one", or bucket ordering across reports.

**Red runs to capture** (each a one-line source edit):

- Delete `frame_metrics::note_keystroke()` in `wire_keys` → assertion 6 fails with `keys=0`.
- Make `frame_metrics::enabled()` return `false` → assertions 2 and 3 fail.
- Delete the `end(Bucket::DrawGrid, …)` at the `paint` call → assertion 8 fails on `draw.grid`.
- Delete the zero-count skip in `summarize` → assertion 9 fails.

## 7. Implementation order

1. `src/frame_stats.rs` with its `#[cfg(test)] mod tests`; declare it in `lib.rs`. `cargo test -p wolf-gnome --lib` green before any glue.
2. `src/frame_metrics.rs`; declare it in `lib.rs`.
3. `src/app.rs`: `install()` and `flush_final()`.
4. `src/window.rs`: `note_wake`, the four wake regions, `maybe_emit`, `note_keystroke`.
5. `src/shell.rs`: `wake.state`.
6. `src/pane_view.rs`: the ten call sites.
7. `cargo build --workspace --all-targets` and `cargo clippy --workspace --all-targets -- -D warnings`.
8. Add the smoke; run it; confirm green.
9. Capture the four red runs of §6.2 and the per-test falsifiers of §6.1.
10. Instrument-side docs.
11. **Measurement** (§9). Then the repair decision.
12. If anything is indicted: repair, re-measure with the identical recipe, re-run the gate, then write the measured reading into `DECISIONS.md` and the prose into `feat-list.md`.
13. `feat-list.md`: tick, branch, prose, renumber.
14. `prettier --write` every edited Markdown file, then `./tools/lint/markdown_check.sh <files>`.
15. Full gate: `cargo fmt --all -- --check`, `prettier --check "**/*.md"`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace --all-targets`, `cargo test --workspace` — all through `nix develop --command`.

## 8. Documentation — exactly which file says what

| Document                             | What it gains, and nothing else                                                                                                                                                                                                                                                                                                                                                                    |
| ------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `build.md`                            | Restructure `## Frame-time instrumentation (macOS)` into `## Frame-time instrumentation` with a shared preamble naming the three variables, then `### macOS` (existing body unchanged) and `### Linux`: the release build and run commands, the `WOLF_FRAME_STATS_FILE` + `tail -f` form, and the `cage` recipe of §9. Link to `crates/wolf-gnome/src/README.md` for the bucket set and to `DECISIONS.md` for the cadence. **Two stale sentences must be fixed**: line 84 ("This front-end carries no frame-time instrumentation, so nothing here claims a speed-up") and line 282 ("`WOLF_FRAME_STATS` … is Swift instrumentation rather than `tracing`"). |
| `crates/wolf-gnome/DECISIONS.md`      | A new `## Frame-time instrumentation` section: the no-timer cadence and its consequence (§2.1), the environment-name parity decision (§2.2), the process-global carve-out (§2.5), the `wakes/key` upper-bound caveat (§2.4), the `eprintln!`-panics rule (§2.6), the one-boolean-test disabled cost, and — after the measurement — a short measured reading naming what was indicted or that nothing was. Add **one** cross-reference clause to "Threading and the wake". |
| `crates/wolf-gnome/src/README.md`     | Two new `####` entries in alphabetical position (between `editor_inset.rs` and `geometry.rs`), each within the ~200-word budget. Edit the existing `coordinator.rs` and `app.rs` entries in place to narrow the "no process-global state" claim.                                                                                                                                                     |
| `crates/wolf-gnome/tests/README.md`   | Extend the `smoke_headless.rs` entry with the new smoke's coverage and gotchas. **Recount the four numbers in that entry from the file itself.** Add to "What is still unproven": the exit-time flush, the file-open failure degradation, the multi-window merge of one accumulator, and that `wakes/key` is an upper bound no assertion can tighten.                                                  |
| `feat-list.md`                        | Tick Linux on the row and set the Branch cell to `feat/slow-typing-latency` / `linux-feat/typing-latency`. One prose paragraph on what the Linux instrument measures and what it does not. If no repair ships, say plainly that the row is ticked on the instrument plus a measurement that indicted nothing. Remove item 1 from "Suggested order of work for Linux" and renumber 2–5 to 1–4.          |
| Not changed                           | `architecture.md`, `crates/wolf-gnome/README.md`, `crates/wolf-macos/*`.                                                                                                                                                                                                                                                                                                                            |

## 9. The measurement (Part B) — run it, then decide

**Binary.** The release profile: `nix develop --command cargo build --release`, run as `./target/release/wolf-gnome`. **Not relocatable** — `app.rs`'s `init_lua_path` bakes `env!("CARGO_MANIFEST_DIR")`. Never measure a `dev` build.

**Driving it headlessly.** No display session on this host, so `cage` plus `wtype`, from the repository root inside `nix develop`:

1. Create a fixture project directory outside the checkout containing one large text file (~2000 lines of realistic prose, so the grid is full) and sibling state/config directories, mirroring `cage_command`, so the run cannot touch the developer's real `~/.local/state/wolf` or `~/.config/wolf`.
2. Launch `cage` with that directory as its working directory, with `HOME` set to the fixture's parent, `DBUS_SESSION_BUS_ADDRESS` removed, and `WOLF_FRAME_STATS=1`, `WOLF_FRAME_STATS_INTERVAL=2`, `WOLF_FRAME_STATS_FILE=/tmp/wolf-frame-stats-<run>.txt`.
3. **Do not set `WOLF_FONT_PT`.** The smokes' 28 pt exists for OCR and would understate every per-cell cost.
4. In the scenario: settle, `wtype -- ":e big.txt"` + Enter, `wtype -- "G"`, then several bursts of `wtype -d 30 -- "<200 characters>"` + Escape over roughly 30 s, then kill.
5. Record, beside the numbers, the grid size (`winwidth(0)` / `winheight(0)`), the font size, the pane count, and whether a preview was open.
6. Run a **second scenario** with a Markdown preview open, because `DECISIONS.md` already records that the preview render is the only blocking RPC that repeats while the user simply types — a pre-registered suspicion.
7. Run each scenario twice. Only a difference larger than the spread between two runs of the same scenario counts as a signal.

**What counts as an indictment** — pre-registered. The budget is one 60 Hz frame, 16.7 ms.

- **R1 (frame budget).** Any top-level bucket — `wake`, `draw.pane`, `draw.cmdline` — with p95 above 8 ms is indicted; above 16.7 ms it is dropping frames on its own.
- **R2 (share).** Inside an indicted parent, the child whose `count × mean` is at least 40 % of the parent's is where the time is. A child below 20 % cannot pay for a repair.
- **R3 (blocking RPC on the typing path).** `prev.render` with a non-zero count in a keystroke-only interval and a mean above 1 ms is an RPC per keystroke, indicted regardless of R1.
- **R4 (redundant work).** `count(draw.pane) ≥ 0.9 × count(wake) × panes` together with a `draw.grid` mean at or above 4 ms indicts the per-wake full-grid repaint.
- **R5 (insufficient resolution).** `pane.wake` minus `pane.state` minus `pane.prev` at or above 50 % of `pane.wake`, or the same shape inside `draw.pane`, is not an indictment: add a bucket, re-measure, do not repair.
- **Ranking.** Rank by `count × mean`; p95 decides whether a frame is dropped, the product decides whether fixing it is worth anything.
- **Non-indictment is a legitimate outcome and is explicitly authorized.** If every top-level p95 is under 8 ms and no rule fires, ship the instrument alone, record the measured table and the conditions, say so in `feat-list.md`'s prose. **Do not port a macOS repair because it exists there.**

**If something is indicted**, the procedure is fixed: design the repair against the named bucket; re-run the identical recipe; the "after" report must show that bucket's `count × mean` down by the claimed factor with no other bucket up; both reports go into the branch's evidence.

## 10. Lint considerations

- `impl Default for FrameStats` alongside `new()` (`clippy::new_without_default`).
- `u64::try_from(d.as_nanos()).unwrap_or(u64::MAX)` rather than `as u64`; inline format arguments.
- Discard write results with `let _ = …`.
- Prefer restructuring to an `#[allow]`.
