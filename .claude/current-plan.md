# Plan: wolf-gnome windowed application (single window, no dialogs)

## Status: COMPLETE ✅ (windowed app shipped)

### Final state
- App: single adw::ApplicationWindow + HeaderBar + DrawingArea; paint via Cairo/Pango;
  EventControllerKey input; resize; semi-transparent block cursor; wake marshalled to GTK
  main thread; clean shutdown. NO file dialogs (open via :e). Verified rendering "hello"
  under cage (real screenshot + OCR).
- Tests: geometry 2✓, gdk_key 1✓, paint_golden 3✓, roundtrip_nvim 1✓, headless smoke ✓(--ignored).
  wolf-core regression 15✓. clippy clean. GUI denylist CLEAN.
- Docs: crate/src/tests READMEs + architecture.md updated; smoke comment fixed.
- New files: src/app.rs, src/geometry.rs, src/main.rs, tests/smoke_headless.rs, tests/testdata/.
- Accepted follow-ups (not done, out of scope): N1 duplicate-activation guard, N2 dedup color consts.

## (history) REVIEWER APPROVED — technical-writer updating docs (final step)

### Reviewer verdict: APPROVED, no Must-fix. All 10 check areas PASS (threading/wake,
borrow discipline, lifecycle/shutdown, draw/cursor, input, resize, scope, architecture,
flake hygiene, clippy). Nits ACCEPTED AS-IS (out of scope / cosmetic):
- N1: connect_activate not guarded against duplicate activation (single-instance second
  launch would spawn 2nd window+nvim). Normal single `cargo run` fine. Follow-up, not now.
- N2: RGB_MAX/white constants duplicated in app.rs and render.rs (render's private). Cosmetic.

## (history) ALL TESTS PASS (incl. headless smoke) — proceeding to reviewer

### Final test run
- wolf-gnome normal: geometry 2✓, gdk_key 1✓, paint_golden 3✓(+regen ign), roundtrip_nvim 1✓.
- wolf-gnome smoke (--ignored): window_renders_typed_text_under_cage ✓ (real app under cage, typed
  "hello", grim screenshot, tesseract --psm 11 OCR confirmed) — passed first try, no flakiness.
- wolf-core regression 15✓. clippy clean. GUI denylist CLEAN.
- TODO for technical-writer: smoke test doc comment (~line 239) still says "tesseract <png> stdout"
  without --psm 11 — update for accuracy.

## (history) CODER DONE (app works; 1 OCR test bug) — test-writer fixing tesseract PSM

### Coder result
- App IMPLEMENTED + LAUNCHES under cage; coder viewed screenshot: "Wolf" headerbar,
  "hello" rendered, block cursor, ~ markers. Full stack (window/paint/input/cursor/resize) works.
- Versions: gtk4 0.11.3, gdk4 0.11.2, libadwaita 0.9.1, glib 0.22.7, cairo/pango 0.22, async-channel 2.5.
- Tests: geometry 2✓, gdk_key 1✓, paint_golden 3✓(+1 ign), roundtrip_nvim 1✓. wolf-core 15✓.
  clippy clean. denylist CLEAN. Build ~22s.

### ORCHESTRATOR DECISIONS
- D1: ACCEPT coder deviations — (a) start WolfCore before building window (honors
  "on start Err, don't build window"); (b) run_with_args::<&str>(&[]) so GApplication
  ignores real argv (bin takes no args).
- D2 (SMOKE TEST BUG): tesseract default PSM(3) returns EMPTY on sparse light-on-dark
  editor text; coder verified --psm 11 (and --psm 6) reliably read "hello" from the REAL
  rendered window (ran twice, deterministic). APPROVED FIX: add `.arg("--psm").arg("11")`
  to the tesseract Command in tests/smoke_headless.rs ocr_to_lowercase. Loose contains("hello")
  assertion stays. This is the ONLY change; the app is correct. Route to test-writer.

### Test-writer output
- src/geometry.rs: placeholder header + #[cfg(test)] mod tests (coder implements grid_size_for ABOVE it).
- tests/smoke_headless.rs: single #[test] #[ignore] window_renders_typed_text_under_cage.

### ORCHESTRATOR DECISION (geometry semantics)
- D-GEO: grid_size_for returns (1,1) as a WHOLE result if EITHER cell size is
  degenerate (0/neg/NaN/inf) OR EITHER dim is non-positive. Coder must implement
  whole-result collapse to match the test (not per-axis). Fine for a resize guard.

### NOTES coder must honor (from test-writer)
- geometry.rs won't compile until grid_size_for(i32,i32,f64,f64)->(u16,u16) is added above tests.
- App must honor WOLF_FONT_PT env override (smoke test sets it to 28) and launch with NO CLI args.
- Must add `pub mod geometry;` to lib.rs AND declare `[[bin]] wolf-gnome` so CARGO_BIN_EXE_wolf-gnome exists.

## Problem statement
Build the actual windowed `wolf-gnome` GTK/Adwaita app, wiring the existing
Tier-1 seams (`gdk_key`, `paint`, `CellMetrics`) into ONE real window. Users open
files via Neovim commands (`:e`); NO file dialogs.

### In scope (requirements)
1. `adw::Application` + `adw::ApplicationWindow` with a header bar + a
   `gtk::DrawingArea` as content.
2. `WolfCore::start` with a wake callback that marshals from the tokio worker
   thread onto the GTK main thread (async-channel + `glib::spawn_future_local`)
   and calls `queue_draw`.
3. `DrawingArea::set_draw_func` reads `core.read_snapshot()` and calls existing
   `paint()` onto the widget's Cairo context using `CellMetrics::for_font`.
4. `gtk::EventControllerKey` -> existing `gdk_key()` -> `core.send_input()`.
5. On DrawingArea resize, a PURE testable fn (pixels -> (cols,rows)) computes the
   grid size; call `core.resize()`.
6. Minimal block cursor drawn at `snapshot.cursor`.
7. Hold `WolfCore` in `Rc<RefCell<Option<WolfCore>>>`; clean `shutdown` on close.

### Deps to add now (were deferred in Tier 1)
- Cargo: `gtk4` (as `gtk`) + `libadwaita` (as `adw`), pinned to the SAME gtk-rs
  release set already in use (gdk4 0.11 / cairo-pango 0.22).
- flake.nix: `libadwaita` + GApplication runtime deps so it actually launches
  (wrapGAppsHook4 or env: gsettings-desktop-schemas, adwaita-icon-theme,
  GSETTINGS_SCHEMA_DIR / XDG_DATA_DIRS).
- Add a `[[bin]]` (`src/main.rs`); keep the lib seams (`lib.rs`) intact.

### Verification (USER-CHOSEN: build + unit glue + headless-compositor smoke)
- Unit-test the PURE resize-geometry function (pixels -> cols/rows).
- Build + clippy clean (via `nix develop --command`).
- Headless-compositor smoke: provision `cage` (headless wlroots), `wtype`,
  `grim`, `tesseract` in the flake; an OPT-IN / ignored-by-default test launches
  the real app under cage, types a key via wtype, captures with grim, OCRs the
  grid to confirm a real window renders.

### Constraints
- Build gate: all builds/tests via `nix develop --command ...` (direnv env stale
  after flake edits).
- Do NOT break wolf-core's GUI denylist — GTK stays in wolf-gnome only.
- Docs are source of truth: update crate/src/tests READMEs + architecture.md so
  they reflect that the window now exists.
- Prefer real nvim; no mocks.

### Existing seams to reuse (do NOT reimplement)
- `wolf_gnome::gdk_key(keyval: gdk::Key, state: gdk::ModifierType) -> Option<KeyEvent>`
- `wolf_gnome::paint(cr: &cairo::Context, snap: &Snapshot, metrics: &CellMetrics)`
- `wolf_gnome::CellMetrics { pub cell_w, pub cell_h, baseline, font }` +
  `CellMetrics::for_font(cr, family, size_pt)`
- Core API: `WolfCore::start/read_snapshot/send_input/resize/shutdown`,
  `CoreConfig { init_lua, cols, rows }`, `WakeFn = Arc<dyn Fn()+Send+Sync>`.

## Progress log
- (orchestrator) Plan reset for windowed-app phase. Tier 1 is COMPLETE (seams +
  tests landed and green). Next: invoke architect.
- (orchestrator) Architect plan recorded below. Version pairing verified:
  gtk4 0.11 + libadwaita 0.9 (no version feature) coherent with gdk4 0.11 /
  cairo-pango 0.22 / system GTK 4.22. Next: test-writer.

---

# ARCHITECT PLAN

## Key decisions
- Add `gtk` (gtk4 0.11), `adw` (libadwaita 0.9, NO version feature), `async-channel 2`
  to wolf-gnome/Cargo.toml. Add `[[bin]] wolf-gnome -> src/main.rs`. Keep [lib] seams.
- New modules: `src/geometry.rs` (PURE), `src/app.rs` (all GTK wiring), `src/main.rs` (thin).
  `lib.rs` adds `pub mod app; pub mod geometry; pub use geometry::grid_size_for;`.
- App id `org.wolf.Wolf`; defaults: font DejaVu Sans Mono 14pt (env `WOLF_FONT_PT` override),
  CoreConfig 80x24, window 960x640.

## flake.nix
LAUNCH-ALWAYS packages: libadwaita, gsettings-desktop-schemas, adwaita-icon-theme,
hicolor-icon-theme, librsvg, shared-mime-info. shellHook (APPEND, keep WOLF_NVIM_BIN +
FONTCONFIG_FILE): export XDG_DATA_DIRS (gsettings-desktop-schemas + gtk4 schemas +
adwaita/hicolor icon shares) AND GSETTINGS_SCHEMA_DIR (both schema dirs). Prefer explicit
env exports over wrapGAppsHook4 for `nix develop --command cargo run` (fallback:
wrapGAppsHook4 in nativeBuildInputs if schema errors persist).
SMOKE-ONLY packages: cage, wtype, grim, tesseract. shellHook smoke env: XDG_RUNTIME_DIR
(default to mktemp if unset), WLR_BACKENDS=headless, WLR_LIBINPUT_NO_DEVICES=1. Do NOT
export WAYLAND_DISPLAY (cage sets it for children). Comment which pkgs are launch vs smoke.

## geometry.rs (PURE, no GTK/pango)
`pub fn grid_size_for(width_px: i32, height_px: i32, cell_w: f64, cell_h: f64) -> (u16,u16)`
(deliberate divergence from &CellMetrics sketch: take f64s so tests need nothing GTK).
cols=floor(w/cell_w), rows=floor(h/cell_h); clamp each to [1, u16::MAX]; non-positive
dims or zero/neg/NaN/inf cell sizes -> (1,1). `#[must_use]`. Cast lints: clamp THEN
convert, narrow #[allow] w/ comment.

## app.rs wiring (build order inside build_ui)
1. window + adw::HeaderBar + DrawingArea. 2. measure CellMetrics ONCE via throwaway 1x1
ImageSurface probe (CellMetrics::for_font), store in Rc<CellMetrics> (shared by draw +
resize; available even if resize fires before first draw). 3. async_channel::bounded(1)
(coalesces flushes). 4. WakeFn = Arc::new(move || { let _=wake_tx.try_send(()); })
(Send+Sync, fires on tokio worker; dropping Full is correct). 5. init_lua =
CARGO_MANIFEST_DIR/../../runtime/nvim/init.lua; WolfCore::start(CoreConfig{80,24}, wake);
store Rc<RefCell<Option<WolfCore>>>. 6. AFTER start: glib::spawn_future_local(async move {
while wake_rx.recv().await.is_ok() { area.queue_draw(); } }); plus one initial queue_draw.
7. set_draw_func, key controller, resize handler.
DRAW: borrow core, read_snapshot(), paint(cr,&snap,&metrics), then block cursor at
cursor.row/col*cell_w/h as semi-transparent filled rect (rgba alpha 0.5 so glyph shows;
color from mode_infos[cursor.mode_idx].attr_id resolved, else default fg/white). Keep
snapshot guard short (drop before closure returns).
INPUT: gtk::EventControllerKey on window; set DrawingArea focusable+grab_focus;
connect_key_pressed -> gdk_key(keyval,state) -> Some => send_input(Key), Propagation::Stop;
None => Proceed. No IMContext.
RESIZE: connect_resize -> grid_size_for(w,h,metrics.cell_w,metrics.cell_h) -> core.resize;
do NOT resize local grid (authoritative size returns via grid_resize).
LIFECYCLE: window.connect_close_request -> take Option out in its own stmt (release
RefCell borrow), then core.shutdown() (consumes self). run(): adw::Application::builder
.application_id(APP_ID), connect_activate(build_ui), app.run(). On WolfCore::start err,
log + don't build window.

## Tests (test-writer)
- geometry: `#[cfg(test)] mod tests` in geometry.rs (pure). Cases: typical
  (800,592,8,16)->(100,37); flooring (17,17,8,16)->(2,1); min-1 clamp for tiny/zero/negative
  dims ->(1,1); degenerate cell sizes 0/neg/NaN/inf ->(1,1); huge dims clamp to u16::MAX.
- smoke: tests/smoke_headless.rs, single `#[test] #[ignore]` (run with --ignored). Locate
  bin via env!("CARGO_BIN_EXE_wolf-gnome"). Run WHOLE scenario inside cage so wtype/grim
  inherit cage's WAYLAND_DISPLAY: `cage -- sh -c '<scenario>'` inheriting env (WOLF_NVIM_BIN,
  WLR_BACKENDS, XDG_RUNTIME_DIR, FONTCONFIG_FILE; set WOLF_FONT_PT=28). Scenario: launch app
  bg, poll/wait ~3s for window, `wtype -- ihello`, `wtype -k Escape`, settle, `grim $OUT_PNG`,
  kill app. Host: `tesseract $OUT_PNG stdout`, lowercase, assert contains "hello" (loose).
  Fail with clear msg if cage/wtype/grim/tesseract absent or XDG_RUNTIME_DIR unwritable.
  Wait loops not fixed sleeps; large font; short lowercase word.
- Tier-1 tests unchanged + must pass; wolf-core 15 must pass; denylist clean.

## Docs (technical-writer later)
crate README (window exists, cargo run cmd, WOLF_FONT_PT, opt-in smoke --ignored + deps),
src README (main.rs/app.rs/geometry.rs), tests README (geometry unit + smoke), architecture.md
wolf-gnome status note -> windowed app exists (window/DrawingArea paint/key input/resize/block
cursor); deferred dialogs/theming/mouse/scroll/native cmdline.

## Risks
Schema/icon discovery = most likely launch failure (mitigate via XDG_DATA_DIRS +
GSETTINGS_SCHEMA_DIR; fallback wrapGAppsHook4). gtk-rs 0.11 vs system GTK 4.22: pin 0.11/0.9
no version feature. WolfCore non-Send: held only in Rc on main thread, only WakeFn crosses
threads; tokio runtime vs glib loop independent (communicate via snapshot lock + channel);
keep read borrows short. cage flakiness: run-all-in-cage + retry loops + loose OCR + #[ignore].
Cursor shape: block for v1. HiDPI: 96-dpi probe ignores scale, deferred.
