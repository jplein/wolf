# Unit: coalescing the Markdown preview render (wolf-gnome)

Branch: `linux-feat/preview-coalesce`, cut from `linux` (e76fe33).

## Problem

Typing beside a live Markdown preview issues a blocking `render_preview` RPC per
keystroke — 988 calls at 6.93 ms mean against 1238 keystrokes, 15.6 % of wall clock,
pushing `wake`'s p95 to 9.1 ms and its worst interval to 34 ms. It is the only blocking
RPC that repeats while the user simply types. `crates/wolf-gnome/DECISIONS.md`,
"Frame-time instrumentation", holds the numbers and the conditions.

## The budget, already decided (do not re-litigate)

- **Leading edge.** The first keystroke after an idle gap renders immediately.
- **150 ms trailing.** A burst collapses to one render after the user pauses.
- **500 ms maximum staleness.** Caps a sustained burst at roughly two refreshes a second.

## Orchestrator decisions on the architect's open questions

1. **Three constants, as planned.** `PREVIEW_TRAILING_DELAY`, `PREVIEW_MAX_STALENESS`
   and `PREVIEW_LEADING_IDLE` all exist by name, with the documented invariant
   `PREVIEW_LEADING_IDLE >= PREVIEW_TRAILING_DELAY` pinned by test 4's preamble. The
   brief requires all three budget elements tunable without reading the algorithm.
2. **Assertion 12 (the render count, read out of a frame-stats report inside the
   existing smoke) is in.** It is the only thing in either suite that can see the wiring.
   If it proves flaky, report it — do not weaken it silently.
3. **The end-to-end measurement is a separate deliverable from assertion 12** and is
   taken by hand per §7.4 below. A number is claimed only if it was measured.
4. This crate has **no Bazel target**; `cargo test` discovers every file in `tests/`.
   No `rust_test` wiring is owed.

---

# Architect's plan (authoritative)

## 2. Facts established from the source

**`PaneInner` survives a tab switch.** One `PaneInner` per tab (`pane_view::build`,
called at `crates/wolf-gnome/src/tab_view.rs:391`), held by `TabInner.panes`
(`tab_view.rs:204`) for the life of the page. `TabsHandle::on_render_wake`
(`tab_view.rs:919-946`) does not call `TabHandle::refresh` for a `TabWake::Hold`
(`tab_view.rs:938`), so a background tab's `refresh_previews` is not called at all.
`PaneInner` is dropped only when the page is deregistered (`tab_view.rs:374-378`).

**`PaneState`/`PaneWidgets` do not survive.** `PaneState` (`pane_view.rs:420`) is a
per-wake value. The `panes: RefCell<HashMap<PaneId, PaneWidgets>>` map is replaced
wholesale by `apply_layout_rebuild` (`pane_view.rs:742`), cleared by
`reset_after_restart` (`pane_view.rs:3205`), and has entries removed by `detect_closes`
(`pane_view.rs:1216`, `:1230`).

**Conclusion: the coalescer's state lives on `PaneInner`, one per tab, never per pane.**

Two further facts:

- `load_preview_document` (`pane_view.rs:2034-2098`) touches only the pane's
  `webkit6::WebView` and `head_len`. A deferred render needs no `queue_redraw`.
- `preview_ui`'s docs already describe the render as "the coalesced `render_preview`"
  (`preview_ui.rs:38`, `:410`). Per CLAUDE.md the implementation is what must move;
  `preview_ui.rs` needs no edit.

## 3. Corrections to the candidate design

1. **`request` must not be called on every wake.** Most wakes have no due pane. The glue
   asks "is any pane due?" first (a cheap `current_load`/`loaded` comparison through
   `preview_ui::needs_reload`, no allocation, no RPC) and calls `request` only then.
   Otherwise a timer would be armed continuously while typing with no preview open.
2. **The `may_render` refusal has two distinct cases.** At wake time the gate is read
   *before* `request`, so a refusal touches no coalescer state. At fire time a refusal
   returns without calling `rendered`, leaving `pending_since` set. State both
   separately. A stale `pending_since` enters only through a `min`, so it can only pull
   a later deadline *earlier* — never later.
3. **No `Instant` arithmetic.** `Instant + Duration` panics on overflow. Express the
   deadline in durations only:
   `delay = PREVIEW_TRAILING_DELAY.min(PREVIEW_MAX_STALENESS.saturating_sub(now.saturating_duration_since(pending_since)))`.
4. **`rendered(now)` is called from exactly one funnel** — the last statement of the one
   private method that performs a render pass — so no caller can forget it.
5. **One fresh one-shot per request** (not a repeating source, not a self-re-arming one).
   Rejected A: `glib::timeout_add_local` with `ControlFlow::Continue` — a genuinely
   repeating source, which reads as exactly the periodic timer `DECISIONS.md:61-63`
   forbids. Rejected B: one live source that re-arms itself for the remaining time — it
   makes `fire` three-way and moves arming into the fire closure, to save a `GSource`
   attach that costs microseconds against a 6.93 ms RPC. The chosen shape bounds live
   sources at `ceil(TRAILING / inter-key interval)` ≈ 5 during a fast burst and **zero
   while idle**, which is the property `DECISIONS.md` states.
6. The tab-gate interaction is the sharpest case — see §6.3.
7. A frame-stats nesting contract breaks unless the fire path opens its own `pane.prev`
   scope — see §5.4.

## 4. New file: `crates/wolf-gnome/src/preview_coalescer.rs`

Pure, GTK-free. No widget type, no `WolfCore`, no clock read of its own — every entry
point takes `now: Instant` from the caller.

**Constants** (the one site in the crate where these numbers live; module docs carry the
literals, `DECISIONS.md` cites the names):

- `pub const PREVIEW_TRAILING_DELAY: Duration = Duration::from_millis(150);` — above a
  fast typist's inter-key interval, below the ~200 ms at which a delayed update reads as
  lag.
- `pub const PREVIEW_MAX_STALENESS: Duration = Duration::from_millis(500);` — a pure
  trailing debounce renders *never* during sustained typing; this caps it at roughly two
  refreshes a second.
- `pub const PREVIEW_LEADING_IDLE: Duration = Duration::from_millis(150);` — the quiet
  since the last render that re-arms the leading edge. **Must never be shorter than
  `PREVIEW_TRAILING_DELAY`**, or a keystroke just after a trailing fire takes the leading
  edge and renders back-to-back with it. Separate because it answers a different
  question, and folding the two would make a retune of the trailing delay silently move
  the leading-edge threshold.

**Types**

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderDecision {
    Render,                                     // render inline on this wake
    Arm { generation: u64, delay: Duration },   // arm a one-shot glib timeout
}

pub struct PreviewCoalescer {
    pending_since: Option<Instant>,  // when the current unrendered burst began
    last_render: Option<Instant>,    // when a pass last actually performed
    armed: bool,
    generation: u64,
}
```

**Functions**

- `pub fn new() -> PreviewCoalescer` + `impl Default`.
- `#[must_use] pub fn request(&mut self, now: Instant) -> RenderDecision`
  - If `pending_since.is_none()`: set it to `now`; if
    `last_render.is_none_or(|t| now.saturating_duration_since(t) >= PREVIEW_LEADING_IDLE)`
    answer `Render` and return.
  - Otherwise compute `delay` per §3.3, set `armed = true`,
    `generation = generation.wrapping_add(1)`, answer `Arm { generation, delay }`. The
    bump on *every* arm is what supersedes the previous source and makes the trailing
    edge track the **last** keystroke rather than the first.
- `pub fn fire(&mut self, generation: u64) -> bool` — `armed && generation == self.generation`;
  clears `armed` **before** returning true (the `RefreshCoalescer::fire` rule).
- `pub fn rendered(&mut self, now: Instant)` — `pending_since = None; last_render = Some(now); armed = false`.

**No `cancel()`.** Module docs must say why: `RefreshCoalescer::cancel` exists because a
re-root makes the armed fire's *content* wrong. Here the fire re-derives the pane set,
the liveness reading and the document from live state at fire time, so there is no wrong
content to cancel, and every teardown path is covered by `Weak::upgrade` and `may_render`.

Module docs must also carry: the relationship to `refresh_coalescer.rs` (same
generation-token mechanism, different rule — do not merge); why the leading edge needs
both conjuncts; why the deadline is a `min` of two terms; the one-shot-per-request
decision with the two rejected alternatives.

## 5. Files to modify

### 5.1 `src/lib.rs`

- `pub mod preview_coalescer;` in the alphabetical list at `:392-394`.
- New seam bullet in the module inventory near `:251-279`.
- `:269`: `the per-wake render` → `the coalesced render`.

### 5.2 `src/pane_view.rs` — the glue

**New field on `PaneInner`** (`:484-569`), near `pending_layout`:
`preview_coalescer: RefCell<PreviewCoalescer>,`. Doc: one per **tab**, never per pane,
because `panes` is replaced by `apply_layout_rebuild` and cleared by
`reset_after_restart` (cite both), so a per-pane coalescer would drop a burst's trailing
edge exactly when a rebuild happened mid-burst. `RefCell` mirrors
`tree_view::TreeInner::coalescer` (`tree_view.rs:192`). Initialise in `build` (`:3399`).

**Split `refresh_previews` (`:1564-1652`) into four.** Keep the existing doc comment's
`seq` warning and call-ordering paragraph on `refresh_previews`.

1. `fn refresh_previews(self: &Rc<Self>)` — keeps `ids` and the `sync_preview_view` /
   `apply_preview_frame` loop (`:1565-1569`) verbatim, then calls
   `self.request_preview_render()`.
2. `fn request_preview_render(self: &Rc<Self>)` — new.
   - The liveness read and `preview_ui::may_render` gate, moved verbatim from
     `:1571-1581` with its comment. A refusal returns having touched no coalescer state.
   - `if !self.any_preview_due() { return; }`.
   - `let decision = self.preview_coalescer.borrow_mut().request(Instant::now());` **in a
     statement of its own**, with a comment: edition 2021 keeps a `match` scrutinee's
     temporary alive for the whole match, so the `Render` arm's re-borrow inside
     `rendered` would be a `RefCell` panic. Same rule as `tree_view.rs:269-270`.
   - `Render => self.run_preview_render_pass()`.
   - `Arm { generation, delay } =>` `let weak = Rc::downgrade(self);` and
     `glib::timeout_add_local_once(delay, move || { ... })`: upgrade or return;
     `let performs = inner.preview_coalescer.borrow_mut().fire(generation);` in its own
     statement; if `performs`, open `let _guard = frame_metrics::scope(Bucket::PanePrev);`
     (a **named** guard — `let _ =` measures nothing) and call
     `inner.run_preview_render_pass()`.
3. `fn any_preview_due(&self) -> bool` — one `panes` borrow, `current_load` vs `loaded`
   through `preview_ui::needs_reload`. No `String` clone, no `preview` /
   `pending_fragment` borrow.
4. `fn run_preview_render_pass(&self)` — holds the existing body from `:1571-1651` in
   order: the liveness read + `may_render` gate re-read here (so a deferred call cannot
   fire into a Neovim that wedged since the arm; a refusal returns **without** calling
   `rendered`), the `DueRender` collection (`:1587-1614` verbatim, including its
   one-borrow comment), the render/load/record loop (`:1615-1651` verbatim, both
   `Bucket` regions intact), and as its **last statement**
   `self.preview_coalescer.borrow_mut().rendered(Instant::now());`. Doc: this is the only
   function in the crate that calls `render_preview` and the only one that calls
   `rendered`; a pass that passes the gate records a render whether or not any pane
   turned out to be due, because "nothing is due" is exactly "there is nothing pending".

**`apply_layout_rebuild` (`:705-777`) is not special-cased** — its `refresh_previews()`
call at `:771` stays as it is. See §6.4.

Imports: `std::time::Instant`, `crate::preview_coalescer::{PreviewCoalescer, RenderDecision}`.

### 5.3 `src/window.rs` and `src/shell.rs` — module docs only

Both state the render is issued "once per changed source per wake — the only one of the
ten that can repeat while the user simply types" (`window.rs:64-67`, `shell.rs:154-158`).
Rewrite both in present tense over the coalesced behaviour: still one of the ten, still
gated by `preview_ui::may_render`, repeating at most once per leading edge plus once per
`PREVIEW_MAX_STALENESS` while typing continues. Replace the clause; do not append.

### 5.4 `src/frame_stats.rs` — module docs only, no code

- Nesting table (`:21-30`): add one sentence that a `pane.prev` region is opened either
  by a pane wake or by the preview coalescer's one-shot fire, so `prev.render` always has
  a parent.
- Count identities (`:48-63`): one bullet in the shape of the existing `count(pane.wake)`
  one — `count(pane.prev)` may exceed `count(pane.wake)`, because a trailing fire is a
  one-shot timer outside every `wake` region.
- `Bucket::PanePrev`'s doc (`:144-145`): the pass, whether inside a pane wake or from the
  coalescer's trailing fire.

## 6. The trailing edge is undroppable — case by case

1. **The burst ends.** The last request armed a fire nothing supersedes; it fires,
   `fire` returns true, the pass re-derives the due set and renders. **This is why it
   must be a timer and not "the next wake":** after the last keystroke there may be no
   further wake at all, so a wake-driven trailing edge is precisely the droppable one.
2. **The window loses focus.** Nothing in this crate ties the preview path to focus, and
   a `glib` one-shot is a main-context source rather than a frame-clock tick.
   *(Standard GLib behaviour, not measured here; the checkable claim is that this crate
   suppresses nothing.)*
3. **A tab switches mid-burst.** `tabs_ui::tab_wake` answers `Hold` and the held tab's
   `on_render_wake` stops being called, so nothing re-arms — but the already-armed source
   belongs to that tab's own `PaneInner` and still fires, bypassing the tab gate.
   Deliberate and safe: the gate protects `pane_status`, which joins the
   current-tabpage-only `Snapshot::windows`, and the render path reads none of it —
   `current_load` comes from the tabpage-global `Snapshot::previews` × `markdown_ticks`,
   and `render_preview` is keyed by bufnr. The cost is bounded to exactly one deferred
   blocking render per held tab. `PaneInner` deliberately gains no reference to the tab
   layer to consult the gate: that coupling would run backwards.
4. **The preview pane closes mid-burst.** `detect_closes` removes the `PaneWidgets`; the
   armed fire re-derives the due set, finds nothing, renders nothing, records `rendered`.
   This is exactly why the fire re-enters the pass instead of capturing a
   `Vec<DueRender>` at arm time.
5. **The window closes.** Two independent belts: the closure holds a `Weak<PaneInner>`,
   and the close handler's `core.borrow_mut().take()` leaves `may_render(None) == false`.
6. **The core is taken out.** Same second belt — which is why `may_render` takes
   `Option<Liveness>` (`preview_ui.rs:62-66`).
7. **`may_render` refuses at fire time.** The pass returns without calling `rendered`.
   `armed` is already cleared, `pending_since` stays set; the next wake with a due pane
   skips the leading edge and computes `delay == Duration::ZERO`, re-arming for the next
   main-loop turn — a one-turn retry, not a lost edit and not a busy loop.

### 6.4 `apply_layout_rebuild` coalesces, with no special case

- A restored preview pane has `loaded == None`, so it is due, and `apply_layout_rebuild`
  only ever runs on a `PaneInner` whose `last_render` is `None`: `realize_layout` runs on
  a pane view built moments earlier (`tab_view.rs:391` → `:620`), and
  `consume_pending_layout` (`pane_view.rs:660-681`) fires on the first wake at which a
  pane exists and early-returns from that wake before `refresh_previews` is reached
  (`pane_view.rs:583-585`). So the leading edge takes it and the restored document loads
  inline, exactly as today.
- A second entry point bypassing the coalescer would be a second place the `may_render`
  gate and the `loaded` bookkeeping must be kept in step.
- If that premise ever stopped holding, the failure mode is a restore whose preview
  paints one trailing delay late — never one that never paints.

## 7. Test plan

### 7.1 Unit tests in `src/preview_coalescer.rs`

House style is `refresh_coalescer.rs:104-286`: a preamble comment stating the timer is
driven **by hand**, nothing sleeps, no constant is compared against its own literal; one
doc comment per test naming the property *and* the revert that must redden it. Build
every `Instant` from one `let t0 = Instant::now();` plus `t0 + Duration::from_millis(..)`.

| # | Test | Behaviour | Revert that must redden it |
| --- | --- | --- | --- |
| 1 | `the_first_edit_after_an_idle_gap_renders_immediately` | `request(t0)` on a fresh coalescer is `Render`; after `rendered(t0)`, `request(t0 + 10s)` is `Render` again. | Delete the early-`Render` branch so `request` always answers `Arm`. Cost: the first keystroke after a pause waits a whole trailing delay for a preview that could have been current at once. |
| 2 | `a_burst_arms_one_fire_and_only_the_newest_generation_performs` | `rendered(t0)`, then `request` at +40/+80/+120 ms answer `Arm` with three **distinct** generations; `fire(g1)`, `fire(g2)` false, `fire(g3)` true. | Stop bumping `generation` in `request`. All three superseded sources then perform — three blocking renders for one burst — and the trailing edge tracks the burst's *first* keystroke. |
| 3 | `the_delay_is_capped_by_the_staleness_deadline` | Requests every 40 ms from a burst beginning at `t0+40`: every `delay <= PREVIEW_TRAILING_DELAY`; at `pending + 400 ms` the delay is exactly `PREVIEW_MAX_STALENESS - 400 ms`; at `pending + PREVIEW_MAX_STALENESS` and beyond it is `Duration::ZERO`. | Drop the `min` against the cap remainder. A continuously typing user then sees no render until they pause, and a burst past the cap waits another full trailing delay. |
| 4 | `an_edit_just_after_a_render_does_not_take_a_second_leading_edge` | `rendered(t0)`; `request(t0+40)` → `Arm{g}`; `fire(g)` true; `rendered(t0+190)`; `request(t0+200)` must be `Arm`, never `Render`. Preamble assertion: `PREVIEW_LEADING_IDLE >= PREVIEW_TRAILING_DELAY`. | Drop the `PREVIEW_LEADING_IDLE` conjunct. Every request then finds `pending_since` empty right after a render and answers `Render`, so every keystroke is its own leading edge and the coalescing is gone — the single most likely edit in this file. **Measured: this revert reddens four of the five tests, not one.** Tests 2, 3 and 5 each open with `rendered(t0)` then `request(t0+40)` and die in the `armed()` helper before reaching their own subject. No smaller edit isolates test 4; the conjunct is exactly what those three rest on too. The test's prose was corrected to say so. |
| 5 | `a_duplicate_fire_at_the_same_generation_performs_nothing` | After a performing `fire(g)`, a second `fire(g)` is false. | Drop the `armed` term from `fire`'s condition. |

Do **not** write a test whose only claim is "a fire the gate refused leaves the burst
recoverable": with a two-variant `RenderDecision` both answers are progress, so no single
edit falsifies it. Test 3's `Duration::ZERO` row is the mechanism that recovery rests on.

### 7.2 Correctness under `cage` — extend the existing smoke

Extend `preview_pane_renders_and_edit_source_returns_under_cage`
(`tests/smoke_headless.rs:16710`) and `PREVIEW_SCENARIO` (`:9770`). **No new smoke, no
new `cage` launch.**

**(a) Make the trailing edge load-bearing.** Step 6 (`:9931-9940`) types the marker at
`wtype`'s default rate, so the whole burst can land inside one flush and be carried by a
single leading-edge render. Change to `wtype -d "$TYPE_DELAY_MS" -- "o$PREVIEW_LIVE_MARKER"`
and add a `TYPE_DELAY_MS=40` knob beside `PREVIEW_SETTLE`, commented with the coupling: it
must stay **below** `preview_coalescer::PREVIEW_TRAILING_DELAY`, or each keystroke becomes
its own leading edge and the trailing edge stops being what puts the marker on screen.
With 13 keys at 40 ms the `o` takes the leading edge, the marker characters coalesce, and
only a fire after the last one carries the complete word — so assertion 11 gains a second
named revert: **neuter the `RenderDecision::Arm` arm of `request_preview_render` so it
arms nothing.** The two reverts are distinguishable from stderr (the single-encoding one
prints `preview body swap failed`, the trailing-edge one prints nothing), and that
belongs in the revert table.

**(b) Guard that (a) stays armed.** New pure guard test beside the nine at `:244-300`:
`preview_burst_delay_stays_below_the_coalescer_trailing_delay`, importing
`wolf_gnome::preview_coalescer::PREVIEW_TRAILING_DELAY` and asserting the smoke's
`TYPE_DELAY_MS` const is strictly below it. Falsifier: raise `TYPE_DELAY_MS` to 200.

**(c) The saving, measured in the same run.** Arm the instrument on this smoke's
`cage_command` (`:16768-16781`): `WOLF_FRAME_STATS=1`, `WOLF_FRAME_STATS_INTERVAL=1`,
`WOLF_FRAME_STATS_FILE=<stats_path>` under `runtime_dir`, passed through
`clear_evidence` alongside the other artifacts (`:16758-16766`). After `grim "$OUT_PNG"`
add `sleep 1.2; wtype -k Escape; sleep 0.3`, commented: reports ride a wake, so the
interval containing the burst is emitted by the *next* wake, and a harmless normal-mode
`Escape` forces it before teardown. Then:

- **Assertion 12 (discriminator): the render count.** Parse with the existing
  `parse_frame_stats_blocks` (`:35053`) and `parse_frame_stats_count` (`:35023`), sum
  `prev.render`'s `count` across blocks, assert `2 <= total <= PREVIEW_RENDER_BUDGET`
  with `PREVIEW_RENDER_BUDGET = 8` documented. Arithmetic for the doc comment: the only
  things moving the source's `changedtick` are the cold open (1) and the 13-key burst;
  coalesced that is 1 open + 1 leading + at most 1 cap + 1 trailing ≈ 3–4, while a
  per-wake render is one per keystroke ≈ 14–15. The lower bound is the non-vacuity half.
  Deliberately **not** a ratio against `keys=`: the scenario types two ~150-character
  `:call writefile(...)` lines that bump no source tick.
- Revert for assertion 12: make `request_preview_render` call `run_preview_render_pass()`
  unconditionally. Assertion 11 stays green under it — which is the point.

**(d) Red-run demonstrations are deliverables** (`tests/README.md:175`): the coder fills
the `measured` column for both new reverts from real red runs.

### 7.3 What stays unproven — record it in `tests/README.md`

- The 500 ms cap has no end-to-end evidence; its only falsifiable evidence is unit test 3.
- The held-tab deferred fire (§6.3) is reasoned from the tab gate's inputs, not measured.

### 7.4 The end-to-end saving (orchestrator's addition)

Separate from assertion 12, and the only thing that may be compared against the 15.6 %
baseline. Per `build.md:377`, on this host (the same `AMD Ryzen AI MAX+ 395` under NixOS
the baseline was taken on):

- Build **two** release binaries: one from `linux` (the uncoalesced baseline) and one
  from this branch. Alternate them in one session, as `DECISIONS.md:341` did.
- Scenario B of the recorded reading: two panes with a live Markdown preview, a large
  prose fixture, `wtype -d 30` bursts over roughly thirty seconds,
  `WOLF_FRAME_STATS_INTERVAL=2`, no `WOLF_FONT_PT`.
- Two runs per binary; report `prev.render`'s count, mean and its share of wall clock,
  plus `wake`'s p95, for each. Anything inside the run-to-run spread is not a signal.
- Keep the driver script in the scratchpad; it is not committed.
- **If it cannot be run, say so plainly and claim no number.**

## 8. Documentation owed

### 8.1 `crates/wolf-gnome/DECISIONS.md`

- `:63` — "Two one-shot timers exist and neither is a poll" becomes three, adding the
  preview's trailing render: armed by an arriving edit, fires once, disarms, none exists
  while idle. Add one sentence stating honestly that during a burst several superseded
  sources can be alive at once, each firing once and performing nothing unless it carries
  the live generation — a reader who finds five queued sources must not conclude the
  invariant is broken.
- **New `##` section immediately after "Threading and the wake"**: the three-part budget
  with the constants **cited by name, not by value**; why a leading edge; why a timer
  rather than the next wake; why a cap; the double read of `may_render` and why a
  deferred blocking call must re-read the gate; the undroppability list of §6 condensed.
- `:289-297` — "Crash and stall recovery". The **count does not change**: ten reachable,
  nine from a running window, all gated on `Alive`. Two edits: `:293`'s closing clause
  about the render being "the only one of the ten that repeats while the user simply
  types" is rewritten to say it is coalesced and what it repeats at; `:297` gains the
  fact that `may_render` alone among the nine is read twice per render, because its call
  can be deferred past the reading that admitted it.
- `:345-347` — "The measured reading". Rewrite in the shape `:341` uses for `draw.grid`:
  keep 988 / 6.93 ms / 15.6 % as the baseline the repair is measured against, add the new
  figures, and **delete outright** the forward-looking sentences at `:347`.

### 8.2 `crates/wolf-gnome/src/README.md`

- **New `#### preview_coalescer.rs` entry** before `#### preview_link.rs` (`:605`).
  Purpose, the public API, and Decisions: the leading edge's two conjuncts; the cap as a
  `min` term; one fresh one-shot per request with a generation token; why it is not
  `refresh_coalescer.rs` widened (that seam is clock-free by construction and is a macOS
  port whose four proven tests would have to be rewritten around a clock, for a rule the
  sidebar does not want); no `cancel`, because the fire re-derives everything.
- `#### pane_view.rs` (`:544-561`): one new Decisions bullet — the render half is
  coalesced, the coalescer is per tab because `panes` is replaced by a layout rebuild,
  and the fire re-enters the same pass so the gate and the due set are re-read. Tighten
  rather than append.
- `#### refresh_coalescer.rs` (`:639-657`): no edit.

### 8.3 `crates/wolf-gnome/tests/README.md`

- `:121-124`: smoke count unchanged at sixty; the pure-guard count goes from nine to ten
  and `69 passed` → `70 passed`.
- The preview smoke's artifact list gains a frame-stats report file — edit in place.
- Add the two "still unproven" entries from §7.3.

### 8.4 `tests/smoke_headless.rs` module docs

`:241-246` "**Nine pure guards**" → ten, with a bullet for the new guard. The frame-stats
smoke's own inventory sentence (`:145-151`) is untouched.

### 8.5 `feat-list.md`

- `:260`: "Only the preview render repeats while the user simply types" — present tense
  over the coalesced behaviour.
- `:268`: replace the "`prev.render` is deliberately left alone … carried onward as a
  unit of its own" sentence with the repaired reading, in the shape of the `draw.grid`
  sentence beside it.
- `:272-278`: delete item 1 of "Suggested order of work for Linux" and renumber "Preview
  in browser" to 1. Change no other wording.

### 8.6 Formatting

`prettier --write` every touched Markdown file, then `./tools/lint/markdown_check.sh`.

## 9. Implementation order

1. `src/preview_coalescer.rs` + unit tests; `lib.rs` wiring.
2. `pane_view.rs`: field, initialiser, the split, the timer arm, the `PanePrev` scope.
3. `frame_stats.rs` doc edits.
4. `window.rs` / `shell.rs` doc clauses.
5. Smoke edits: knob, `-d` pacing, flush key, frame-stats env, assertion 12, new guard.
6. The two red-run demonstrations; fill the revert table's `measured` column.
7. The end-to-end measurement (§7.4).
8. Documentation (§8), with the measured numbers in hand.

## 11. Lint

- `cargo clippy --workspace --all-targets -- -D warnings` covers `#[cfg(test)]` too.
- `impl Default` required. `Option::is_none_or`, not `map_or(true, ..)`.
- No `Instant + Duration`; `saturating_duration_since` / `Duration::saturating_sub` only.
- `#[must_use]` on `request` and `new`.
- Named guard for `frame_metrics::scope`, never `let _ =`.
- No `#[allow]`. If `run_preview_render_pass` trips `too_many_lines`, split the
  `DueRender` collection into `fn due_previews(&self) -> Vec<DueRender>`.

## 12. Flagged / unverified

- GLib timeouts firing for an unfocused toplevel is standard behaviour, not measured here.
- Assertion 12's expected counts (3–4 coalesced, 14–15 uncoalesced) are derived, not
  measured. Confirm both ends from a real run; adjust `PREVIEW_RENDER_BUDGET` only if the
  *coalesced* count exceeds it, never to accommodate an uncoalesced one.
- Whether the report covering the burst reaches the file before teardown depends on the
  added flush key. If flaky, the fix is a longer post-capture idle, not a weaker
  assertion.

---

## The measurement, taken (pass 3) — the numbers the docs must use

Four runs alternated in one session, release profile, both binaries measured rather than
comparing this branch against a recorded number. Independently re-derived by the
orchestrator from the retained reports.

| | baseline (`linux`) | this branch |
| --- | --- | --- |
| `prev.render` count | 1186 / 1188 | 90 / 90 |
| `prev.render` total | 6490.6 / 6527.1 ms | 201.9 / 210.2 ms |
| wall clock | 54.8 / 54.8 s | 55.1 / 55.2 s |
| share of wall clock | 11.84 % / 11.91 % | 0.37 % / 0.38 % |
| `wake` p95 median | 8.55 / 8.50 ms | 0.15 / 0.22 ms |
| worst `wake` max | 30.74 / 34.00 ms | 32.78 / 33.64 ms |

**Three framing constraints, in the coder's words, binding on every doc:**

1. **The baseline reads 11.9 % whole-run, not the recorded 15.6 %.** The shape reproduces
   (1187 renders against 988, `wake` p95 8.5 against 9.1 ms, worst interval 30.7–34.0
   against 34 ms) but the render mean is 5.48 against 6.93 ms, from a denominator carrying
   ~22 s of non-typing wall clock and a generated rather than byte-identical prose fixture.
   **No doc may state the improvement as "15.6 % → 0.4 %"**, which would silently mix two
   measurement setups. Measuring both binaries in one session is what makes the comparison
   sound.
2. **The 34 ms worst interval does not improve, and must not be claimed.** In all four runs
   `wake`'s max lives in the cold preview open, where `prev.load` alone is 10.3–10.9 ms for
   the WebProcess spawn and the full `load_html`. Coalescing does not touch the cold open.
   The p95 median is the statistic to quote.
3. **Total main-thread busy falls by about 3.5 points, not 11.5.** `draw.pane` count rises
   78 % over the same wall clock at an unchanged per-frame mean — frames GTK wanted to
   deliver and could not while the thread sat in a blocking RPC. Summed families go 29.7 %
   → 26.1 %. The honest sentence: the blocking RPC is gone and the wake path is 98 %
   cheaper, and the freed CPU is largely spent painting a smoother editor rather than going
   idle.

The prediction given when the budget was set (15.6 % → roughly 1.4 %) is **not** to be
quoted as confirmed. Quote the measurement.

The staleness cap has value-level evidence from the same run. Across all 28 typing blocks
of both branch runs the modal reading is **4 renders per 2-second block** (24 of 28; range
3–5, the fives being blocks that also caught an inter-burst trailing fire). Four per two
seconds is exactly two a second, which is exactly `PREVIEW_MAX_STALENESS`. An earlier
report of "3 per 2-second block" was one block from one run and must not be used.

## Status

- [x] Branch created
- [x] Architect plan
- [x] Tests (unit + cage, including the sustained-burst cap evidence)
- [x] Implementation, with five unit falsifiers demonstrated red
- [x] Measurement (pass 3)
- [ ] Smoke bounds confirmed (pass 4)
- [ ] Review
- [ ] Docs
- [ ] Full suite, commit, merge
